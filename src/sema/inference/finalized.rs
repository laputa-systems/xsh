use super::*;
use std::mem::size_of;
use std::ops::Deref;

#[derive(Clone, Copy, Debug)]
pub struct ScopedRoot { pub ty: TypeId, pub scope: Option<SchemeId> }

/// The context has no mutable accessor after publication. Its bound handles
/// retain exactly the graph that owns their constructors and generic scopes.
#[derive(Debug)]
pub struct SolvedGraph { context: InferenceContext, publication: PublicationProof }

#[derive(Clone, Copy, Debug)]
pub struct ScopedEffectRoot { pub effect: EffectSummary, pub scope: Option<SchemeId> }

/// Certificates inherit the lexical scope of the source operation. Selecting
/// a catalog candidate does not authorize its caller to use foreign binders.
#[derive(Clone, Copy, Debug)]
pub struct ScopedRequirementRoot { pub requirement: RequirementId, pub scope: Option<SchemeId> }

/// The exact instantiation handles survive publication so source facts can
/// prove their declaration template without repeating inference.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct InstanceCertificate {
    pub scheme: SchemeId, pub signature: TypeId, pub substitutions: Vec<TypeId>,
    pub effect_substitutions: Vec<EffectId>, pub effect_roots: Vec<EffectSummary>,
    pub requirement_origins: Vec<(RequirementId, RequirementId)>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ScopedInstanceRoot { pub certificate: InstanceCertificate, pub scope: Option<SchemeId> }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum PublicationScope { Owned, Lexical(Option<SchemeId>) }

#[derive(Debug, Default)]
struct ScopeAuthorization { types: FxHashSet<TypeId>, effects: FxHashSet<EffectSummary> }

#[derive(Debug, Default)]
struct PublicationProof {
    heights: FxHashMap<(TypeId, PublicationScope), usize>,
    resolutions: FxHashMap<TypeId, (TypeId, usize)>,
    scopes: FxHashMap<SchemeId, ScopeAuthorization>,
    requirements: FxHashSet<(RequirementId, PublicationScope)>,
    instances: FxHashSet<ScopedInstanceRoot>,
    applications: FxHashMap<ApplicationSource, ScopedApplicationRoot>,
    application_counts: FxHashMap<(crate::source::SourceId,Option<Name>,crate::syntax::arena::ExprId),usize>,
    native_contracts: FxHashSet<(NativeContractId, PublicationScope)>,
    native_families: FxHashSet<(NativeFamilyContractId, PublicationScope)>,
}

#[derive(Default)]
struct PublishedSubstitutions {
    types: FxHashMap<TypeId, TypeId>,
    effects: FxHashMap<EffectSummary, EffectSummary>,
}

impl SolvedGraph {
    pub fn validate_native_family_contract_scoped(&self, contract: NativeFamilyContractId, scope: Option<SchemeId>) -> Result<(), InferenceError> {
        self.native_family_origin(contract)?; if let Some(scope) = scope { self.scheme(scope)?; }
        if self.publication.native_families.contains(&(contract, PublicationScope::Lexical(scope))) { Ok(()) } else { Err(InferenceError::Boundary("native family has no published source proof")) }
    }
    pub fn validate_native_contract_scoped(&self, contract: NativeContractId, scope: Option<SchemeId>) -> Result<(), InferenceError> {
        self.native_contract_origin(contract)?;
        if let Some(scope) = scope { self.scheme(scope)?; }
        if self.publication.native_contracts.contains(&(contract, PublicationScope::Lexical(scope))) { Ok(()) } else { Err(InferenceError::Boundary("native contract has no published source proof")) }
    }
    pub fn validate_application_root(&self, root:&ScopedApplicationRoot) -> Result<(),InferenceError> {
        self.validate_application_root_shape(root)?;
        if self.publication.applications.get(&root.certificate.source)==Some(root) {Ok(())} else {Err(InferenceError::InvalidScheme)}
    }
    pub fn validate_application_source_roots(&self, source:crate::source::SourceId, namespace:Option<Name>, expression:crate::syntax::arena::ExprId, roots:&[ScopedApplicationRoot]) -> Result<(),InferenceError> {
        let count=self.publication.application_counts.get(&(source,namespace,expression)).copied().unwrap_or(0);
        if count!=roots.len() {return Err(InferenceError::InvalidScheme)}
        let mut seen=FxHashSet::default();
        for root in roots {let identity=&root.certificate.source;if (identity.source,identity.namespace,identity.expression)!=(source,namespace,expression)||!seen.insert(identity) {return Err(InferenceError::InvalidScheme)}self.validate_application_root(root)?;}
        Ok(())
    }
    pub fn validate_application_roots(&self, roots:&[ScopedApplicationRoot]) -> Result<(),InferenceError> {
        if roots.len() != self.publication.applications.len() {return Err(InferenceError::InvalidScheme)}
        let mut seen=FxHashSet::default();
        for root in roots {self.validate_application_root_shape(root)?;if !seen.insert(&root.certificate.source) || self.publication.applications.get(&root.certificate.source)!=Some(root) {return Err(InferenceError::InvalidScheme)}}
        Ok(())
    }
    pub fn validate_instance_scoped(&self, root: &ScopedInstanceRoot) -> Result<(), InferenceError> {
        self.scheme(root.certificate.scheme)?; self.node(root.certificate.signature)?;
        if self.publication.instances.contains(root) { Ok(()) } else { Err(InferenceError::Boundary("instance has no published declaration proof")) }
    }
    pub fn validate_requirement_scoped(&self, root: ScopedRequirementRoot) -> Result<(), InferenceError> {
        self.requirement(root.requirement)?;
        if let Some(scope) = root.scope { self.scheme(scope)?; }
        if self.publication.requirements.contains(&(root.requirement, PublicationScope::Lexical(root.scope))) { Ok(()) }
        else { Err(InferenceError::Boundary("requirement has no published source scope")) }
    }
    pub fn validate_scoped(&self, root: ScopedRoot) -> Result<(), InferenceError> {
        let ty = self.publication.resolutions.get(&root.ty).map(|(ty, _)| *ty).map(Ok).unwrap_or_else(|| self.resolved(root.ty))?;
        if self.publication.heights.contains_key(&(ty, PublicationScope::Lexical(root.scope))) { return Ok(()); }
        self.context.validate_scoped(root)
    }
    pub fn retained_storage(&self) -> RetainedStorage {
        let mut storage = self.context.retained_storage();
        storage.validation_capacity = self.publication.heights.capacity() + self.publication.resolutions.capacity() + self.publication.scopes.capacity() + self.publication.requirements.capacity() + self.publication.instances.capacity() + self.publication.native_contracts.capacity() + self.publication.native_families.capacity() + self.publication.applications.capacity() + self.publication.application_counts.capacity();
        storage.validation_bytes += self.publication.instances.capacity() * size_of::<ScopedInstanceRoot>();
        for instance in &self.publication.instances { storage.validation_bytes += instance_certificate_bytes(&instance.certificate); }
        storage.validation_bytes += self.publication.application_counts.capacity()*size_of::<((crate::source::SourceId,Option<Name>,crate::syntax::arena::ExprId),usize)>();
        storage.validation_bytes += self.publication.applications.capacity() * size_of::<(ApplicationSource,ScopedApplicationRoot)>();
        for (source,root) in &self.publication.applications {storage.validation_bytes += source.path.capacity()*size_of::<ApplicationPathComponent>() + applications::application_certificate_bytes(root);}
        for scope in self.publication.scopes.values() { storage.validation_capacity += scope.types.capacity() + scope.effects.capacity(); }
        storage
    }
}

impl Deref for SolvedGraph { type Target = InferenceContext; fn deref(&self) -> &Self::Target { &self.context } }

#[derive(Clone, Copy, Debug, Default)]
pub struct RetainedStorage {
    pub types: usize, pub rows: usize, pub variables: usize, pub schemes: usize, pub reasons: usize,
    pub candidates: usize, pub families: usize, pub operation_calls: usize, pub invocation_calls: usize, pub error_joins:usize, pub native_contracts: usize, pub native_families: usize, pub requirements: usize, pub effects: usize, pub origins: usize, pub trail: usize, pub watchers: usize,
    pub type_capacity: usize, pub row_capacity: usize, pub variable_capacity: usize,
    pub type_bytes: usize, pub row_bytes: usize, pub variable_bytes: usize, pub scheme_bytes: usize,
    pub reason_bytes: usize, pub requirement_bytes: usize, pub effect_bytes: usize,
    pub candidate_bytes: usize, pub family_bytes: usize, pub operation_bytes: usize, pub invocation_bytes: usize, pub native_contract_bytes: usize, pub native_family_bytes: usize, pub error_join_bytes:usize,
    pub origin_bytes: usize, pub trail_bytes: usize, pub queue_bytes: usize, pub interner_capacity: usize, pub legacy_variable_capacity: usize, pub validation_capacity: usize, pub validation_bytes: usize,
}

impl RetainedStorage {
    /// Hash-table allocator overhead is reported separately as a capacity;
    /// vector ownership is counted once, including retained unused capacity.
    pub fn vector_bytes(self) -> usize { self.type_bytes + self.row_bytes + self.variable_bytes + self.scheme_bytes + self.reason_bytes + self.requirement_bytes + self.effect_bytes + self.candidate_bytes + self.family_bytes + self.operation_bytes + self.invocation_bytes + self.native_contract_bytes + self.native_family_bytes + self.error_join_bytes + self.origin_bytes + self.trail_bytes + self.queue_bytes + self.validation_bytes }
}

impl InferenceContext {
    pub fn retained_storage(&self) -> super::RetainedStorage {
        let mut storage = RetainedStorage {
            candidates: self.candidates.len(), families: self.families.len(), operation_calls: self.operation_calls.len(), invocation_calls: self.invocation_calls.len(), error_joins:self.error_joins.len(), native_contracts: self.native_contracts.len(), native_families: self.native_families.len(), types: self.nodes.len(), rows: self.rows.len(), variables: self.metas.len(), schemes: self.schemes.len(), reasons: self.reasons.len(), requirements: self.requirements.len(), effects: self.effects.len(), origins: self.origins.len(), trail: self.trail.len(),
            type_capacity: self.nodes.capacity(), row_capacity: self.rows.capacity(), variable_capacity: self.metas.capacity(),
            candidate_bytes: self.candidates.capacity() * size_of::<Slot<CandidateTemplate>>(), family_bytes: self.families.capacity() * size_of::<Slot<Vec<CandidateId>>>(), operation_bytes: self.operation_calls.capacity() * size_of::<Slot<OperationCall>>(), type_bytes: self.nodes.capacity() * size_of::<Slot<TypeNode>>(), row_bytes: self.rows.capacity() * size_of::<Slot<Row>>(), variable_bytes: self.metas.capacity() * size_of::<Slot<Meta>>(), scheme_bytes: self.schemes.capacity() * size_of::<Slot<Scheme>>(), reason_bytes: self.reasons.capacity() * size_of::<Slot<Reason>>(), requirement_bytes: self.requirements.capacity() * size_of::<Slot<Requirement>>(), effect_bytes: self.effects.capacity() * size_of::<Slot<EffectVariable>>(), origin_bytes: self.origins.capacity() * size_of::<ConstraintOrigin>(), trail_bytes: self.trail.capacity() * size_of::<Trail>(), queue_bytes: self.queue.capacity() * size_of::<RequirementId>(), interner_capacity: self.atoms.capacity(), legacy_variable_capacity: self.legacy_variables.capacity(), ..RetainedStorage::default()
        };
        storage.error_join_bytes = self.error_joins.capacity()*size_of::<Slot<ErrorJoin>>() + self.error_joins.iter().map(|slot|slot.value.inputs.capacity()*size_of::<TypeId>()).sum::<usize>();
        storage.invocation_bytes = self.invocation_calls.capacity() * size_of::<Slot<InvocationCall>>();
        storage.native_contract_bytes = self.native_contracts.capacity() * size_of::<Slot<NativeContract>>();
        storage.native_family_bytes = self.native_families.capacity() * size_of::<Slot<NativeFamilyContract>>() + self.native_families.iter().map(|slot| slot.value.members.capacity() * size_of::<NativeContractId>()).sum::<usize>();
        for slot in &self.native_contracts { let instance = &slot.value.instance; storage.native_contract_bytes += instance.requirement_origins.capacity() * size_of::<(RequirementId, RequirementId)>() + instance.effect_roots.capacity() * size_of::<EffectSummary>() + instance.requirements.capacity() * size_of::<RequirementId>() + instance.substitutions.capacity() * size_of::<TypeId>() + instance.effect_substitutions.capacity() * size_of::<EffectId>(); }
        for slot in &self.nodes { match &slot.value { TypeNode::Arrow(arrow) => storage.type_bytes += arrow.params.capacity() * size_of::<Parameter>(), TypeNode::Module(fields) => storage.type_bytes += fields.capacity() * size_of::<ModuleField>(), TypeNode::NativeCallable(callable) => storage.type_bytes += callable.alternatives.capacity() * size_of::<CallableAuthority>(), TypeNode::FiniteDomain(alternatives) => storage.type_bytes += alternatives.capacity() * size_of::<DomainAlternative>(), TypeNode::CallableChoice(signatures) => storage.type_bytes += signatures.capacity() * size_of::<TypeId>(), _ => {} } }
        for slot in &self.rows { storage.row_bytes += slot.value.fields.capacity() * size_of::<RowField>(); }
        for slot in &self.metas { storage.variable_bytes += meta_owned_bytes(&slot.value); storage.watchers += slot.value.watchers.len(); }
        for slot in &self.schemes { storage.scheme_bytes += slot.value.captures.capacity() * size_of::<TypeId>() + (slot.value.effect_captures.capacity() + slot.value.effect_binders.capacity() + slot.value.effect_roots.capacity()) * size_of::<EffectSummary>() + slot.value.quantifiers.capacity() * size_of::<Quantifier>() + slot.value.requirements.capacity() * size_of::<RequirementTemplate>() + slot.value.requirement_origins.capacity() * size_of::<RequirementId>() + slot.value.binders.capacity() * size_of::<TypeId>() + slot.value.effect_quantifiers.capacity() * size_of::<EffectQuantifier>() + slot.value.effect_inclusions.capacity() * size_of::<(EffectSummary, EffectSummary)>(); for quantifier in &slot.value.quantifiers { storage.scheme_bytes += quantifier.lacks.capacity() * size_of::<Name>(); } }
        for slot in &self.candidates { storage.candidate_bytes += slot.value.actual_eligibility.capacity() * size_of::<(usize, Eligibility)>() + slot.value.argument_relations.capacity() * size_of::<ArgumentRelation>() + slot.value.effect_roles.capacity() * size_of::<(EffectRole, EffectRoleReference)>() + slot.value.output_effect_roles.capacity() * size_of::<(ProducerRole, u32)>(); }
        for slot in &self.families { storage.family_bytes += slot.value.capacity() * size_of::<CandidateId>(); }
        for slot in &self.operation_calls { storage.operation_bytes += slot.value.arguments.capacity() * size_of::<Option<TypeId>>() + slot.value.effect_bindings.capacity() * size_of::<(EffectRole, EffectSummary)>() + slot.value.output_effect_bindings.capacity() * size_of::<(ProducerRole, EffectSummary)>(); }
        for slot in &self.invocation_calls { storage.invocation_bytes += slot.value.arguments.capacity() * size_of::<InvocationArgument>(); }
        for slot in &self.requirements { storage.requirement_bytes += requirement_owned_bytes(&slot.value); }
        for slot in &self.effects { storage.effect_bytes += effect_owned_bytes(&slot.value); storage.watchers += slot.value.watchers.len(); }
        for trail in &self.trail { match trail { Trail::Meta(_, meta) => storage.trail_bytes += meta_owned_bytes(meta), Trail::Effect(_, effect) => storage.trail_bytes += effect_owned_bytes(effect), Trail::Requirement(_, requirement) => storage.trail_bytes += requirement_owned_bytes(requirement), _ => {} } }
        storage
    }
    pub fn validate_scoped(&self, root: ScopedRoot) -> Result<(), InferenceError> {
        let mut proof = PublicationProof::default();
        self.validate_readonly(root.ty, PublicationScope::Lexical(root.scope), &mut proof, 0).map(|_| ())
    }
    fn validate_readonly(&self, root: TypeId, scope: PublicationScope, proof: &mut PublicationProof, depth: usize) -> Result<usize, InferenceError> {
        if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
        let ty = self.resolved(root)?;
        if let Some(height) = proof.heights.get(&(ty, scope)) {
            if depth + height > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
            return Ok(*height);
        }
        self.validate_constructor(ty, scope, proof)?;
        let mut height = 0;
        for child in self.children(ty)? { height = height.max(1 + self.validate_readonly(child, scope, proof, depth + 1)?); }
        proof.heights.insert((ty, scope), height);
        Ok(height)
    }
    fn authorize_scope(&self, scope: SchemeId) -> Result<ScopeAuthorization, InferenceError> {
        let scheme = self.scheme(scope)?;
        let mut authorization = ScopeAuthorization::default();
        for binder in &scheme.binders { authorization.types.insert(self.resolved(*binder)?); }
        let mut pending: Vec<_> = scheme.captures.iter().copied().map(|ty| (ty, 0usize)).collect();
        let mut seen = FxHashSet::default();
        while let Some((ty, depth)) = pending.pop() {
            if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
            let ty = self.resolved(ty)?;
            if !seen.insert(ty) { continue; }
            authorization.types.insert(ty);
            for child in self.children(ty)? { pending.push((child, depth + 1)); }
        }
        for effect in scheme.effect_binders.iter().chain(&scheme.effect_captures) { authorization.effects.insert(self.resolved_effect_summary(*effect)?); }
        Ok(authorization)
    }
    fn validate_constructor(&self, ty: TypeId, scope: PublicationScope, proof: &mut PublicationProof) -> Result<(), InferenceError> {
        if let PublicationScope::Lexical(Some(member)) = scope {
            if !proof.scopes.contains_key(&member) { proof.scopes.insert(member, self.authorize_scope(member)?); }
        }
        match self.node(ty)? {
            TypeNode::Meta(_) => return Err(InferenceError::Unresolved(ty)),
            TypeNode::Poison | TypeNode::NonCompletion => return Err(InferenceError::Recovery(ty)),
            TypeNode::Rigid { scope: owner, index, kind } => {
                let quantifier = self.scheme(*owner)?.quantifiers.get(*index as usize).ok_or(InferenceError::InvalidScheme)?;
                if quantifier.kind != *kind { return Err(InferenceError::KindMismatch); }
                match scope {
                    PublicationScope::Owned => {},
                    PublicationScope::Lexical(Some(member)) if proof.scopes[&member].types.contains(&ty) => {},
                    _ => return Err(InferenceError::ScopeEscape),
                }
            }
            TypeNode::Arrow(arrow) => self.validate_effect(arrow.effects, scope, proof)?,
            _ => {},
        }
        Ok(())
    }
    fn validate_effect(&self, effect: EffectSummary, scope: PublicationScope, proof: &mut PublicationProof) -> Result<(), InferenceError> {
        let effect = self.resolved_effect_summary(effect)?;
        if let PublicationScope::Lexical(Some(member)) = scope {
            if !proof.scopes.contains_key(&member) { proof.scopes.insert(member, self.authorize_scope(member)?); }
        }
        if matches!(effect, EffectSummary::Rigid { .. }) {
            match scope {
                PublicationScope::Owned => {},
                PublicationScope::Lexical(Some(member)) if proof.scopes[&member].effects.contains(&effect) => {},
                _ => return Err(InferenceError::ScopeEscape),
            }
        }
        Ok(())
    }
    fn publication_resolved(&mut self, mut ty: TypeId, proof: &mut PublicationProof) -> Result<TypeId, InferenceError> {
        let mut path = Vec::new();
        for _ in 0..=self.limits.structural_depth {
            if let Some((resolved, suffix_depth)) = proof.resolutions.get(&ty).copied() {
                let total_depth = path.len() + suffix_depth;
                if total_depth > self.limits.structural_depth { return Err(InferenceError::Limit("substitution depth")); }
                for (index, raw) in path.into_iter().enumerate() { proof.resolutions.insert(raw, (resolved, total_depth - index)); }
                return Ok(resolved);
            }
            self.work()?;
            path.push(ty);
            let TypeNode::Meta(mut id) = *self.node(ty)? else {
                let depth = path.len() - 1;
                for (index, raw) in path.into_iter().enumerate() { proof.resolutions.insert(raw, (ty, depth - index)); }
                return Ok(ty);
            };
            let mut root = None;
            for _ in 0..=self.limits.structural_depth {
                self.work()?;
                let meta = self.meta(id)?;
                if meta.parent == id { root = Some((meta.binding, meta.ty)); break; }
                id = meta.parent;
            }
            let Some((binding, representative)) = root else { return Err(InferenceError::Limit("variable depth")); };
            if let Some(binding) = binding { ty = binding; }
            else {
                let depth = path.len() - 1;
                for (index, raw) in path.into_iter().enumerate() { proof.resolutions.insert(raw, (representative, depth - index)); }
                return Ok(representative);
            }
        }
        Err(InferenceError::Limit("substitution depth"))
    }
    fn publish_root(&mut self, root: TypeId, scope: PublicationScope, proof: &mut PublicationProof) -> Result<(), InferenceError> {
        enum Frame { Enter(TypeId, usize), Exit(TypeId, Vec<TypeId>) }
        let mut pending = vec![Frame::Enter(root, 0)];
        let mut active = FxHashSet::default();
        while let Some(frame) = pending.pop() {
            match frame {
                Frame::Enter(raw, depth) => {
                    self.work()?;
                    if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
                    let ty = self.publication_resolved(raw, proof)?;
                    if let Some(height) = proof.heights.get(&(ty, scope)) {
                        if depth + height > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
                        continue;
                    }
                    self.prepare_scope(scope, proof)?;
                    self.validate_constructor(ty, scope, proof)?;
                    if let TypeNode::NativeCallable(callable) = self.clone_node(ty)? { self.publish_native_callable(&callable, scope, proof)?; }
                    if !active.insert(ty) { return Err(InferenceError::InvalidScheme); }
                    let children = self.children(ty)?;
                    self.work_many(children.len())?;
                    pending.push(Frame::Exit(ty, children.clone()));
                    for child in children { pending.push(Frame::Enter(child, depth + 1)); }
                }
                Frame::Exit(ty, children) => {
                    let mut height = 0;
                    for child in children { let child = self.publication_resolved(child, proof)?; height = height.max(1 + proof.heights[&(child, scope)]); }
                    active.remove(&ty);
                    proof.heights.insert((ty, scope), height);
                }
            }
        }
        Ok(())
    }
    fn prepare_scope(&mut self, scope: PublicationScope, proof: &mut PublicationProof) -> Result<(), InferenceError> {
        let PublicationScope::Lexical(Some(member)) = scope else { return Ok(()) };
        if proof.scopes.contains_key(&member) { return Ok(()); }
        let scheme = self.scheme(member)?;
        let binders = scheme.binders.clone(); let captures = scheme.captures.clone();
        let effects: Vec<_> = scheme.effect_binders.iter().chain(&scheme.effect_captures).copied().collect();
        self.work_many(binders.len() + captures.len() + effects.len())?;
        let mut authorization = ScopeAuthorization::default();
        for binder in binders { authorization.types.insert(self.publication_resolved(binder, proof)?); }
        let mut pending: Vec<_> = captures.into_iter().map(|ty| (ty, 0usize)).collect();
        while let Some((ty, depth)) = pending.pop() {
            self.work()?;
            if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
            let ty = self.publication_resolved(ty, proof)?;
            if !authorization.types.insert(ty) { continue; }
            let children = self.children(ty)?;
            self.work_many(children.len())?;
            for child in children { pending.push((child, depth + 1)); }
        }
        for effect in effects { authorization.effects.insert(self.resolved_effect_summary(effect)?); }
        proof.scopes.insert(member, authorization);
        Ok(())
    }
    /// Unspecialized declaration roots retain their owning generic scopes;
    /// executable owners additionally validate their exact lexical member.
    pub fn freeze(mut self, roots: &[TypeId]) -> Result<SolvedGraph, InferenceError> {
        self.solve()?;
        let mut proof = PublicationProof::default();
        for root in roots { self.publish_root(*root, PublicationScope::Owned, &mut proof)?; }
        self.finish_freeze(proof)
    }
    pub fn freeze_scoped(self, roots: &[ScopedRoot]) -> Result<SolvedGraph, InferenceError> {
        self.freeze_scoped_with_effects(roots, &[])
    }
    pub fn freeze_scoped_with_effects(self, roots: &[ScopedRoot], effects: &[ScopedEffectRoot]) -> Result<SolvedGraph, InferenceError> {
        self.freeze_scoped_with_facts(roots, effects, &[])
    }
    pub fn freeze_scoped_with_facts(self, roots: &[ScopedRoot], effects: &[ScopedEffectRoot], requirements: &[ScopedRequirementRoot]) -> Result<SolvedGraph, InferenceError> {
        self.freeze_scoped_with_instances(roots, effects, requirements, &[])
    }
    pub fn freeze_scoped_with_instances(self, roots: &[ScopedRoot], effects: &[ScopedEffectRoot], requirements: &[ScopedRequirementRoot], instances: &[ScopedInstanceRoot]) -> Result<SolvedGraph, InferenceError> {
        self.freeze_scoped_with_applications(roots,effects,requirements,instances,&[])
    }
    pub fn freeze_scoped_with_applications(mut self, roots: &[ScopedRoot], effects: &[ScopedEffectRoot], requirements: &[ScopedRequirementRoot], instances: &[ScopedInstanceRoot], applications: &[ScopedApplicationRoot]) -> Result<SolvedGraph, InferenceError> {
        self.solve()?;
        let mut proof = PublicationProof::default();
        for root in roots { self.publish_root(root.ty, PublicationScope::Lexical(root.scope), &mut proof)?; }
        for root in effects {
            self.work()?; let scope = PublicationScope::Lexical(root.scope);
            self.prepare_scope(scope, &mut proof)?;
            self.validate_effect(root.effect, scope, &mut proof)?;
        }
        for root in requirements { self.publish_requirement(*root, &mut proof)?; }
        for root in instances { self.publish_instance(root, &mut proof)?; }
        for root in applications {
            self.work_many(applications::application_certificate_work(root).saturating_mul(2))?;
            self.validate_application_root_shape(root)?;
            if let Some(previous)=proof.applications.get(&root.certificate.source) {if previous!=root {return Err(InferenceError::InvalidScheme)}continue;}
            let scope=PublicationScope::Lexical(root.scope);self.prepare_scope(scope,&mut proof)?;
            for argument in &root.certificate.arguments {self.publish_root(*argument,scope,&mut proof)?;}
            self.work()?;let identity=&root.certificate.source;
            *proof.application_counts.entry((identity.source,identity.namespace,identity.expression)).or_default()+=1;
            proof.applications.insert(identity.clone(),root.clone());
        }
        self.finish_freeze(proof)
    }
    fn publish_instance(&mut self, root: &ScopedInstanceRoot, proof: &mut PublicationProof) -> Result<(), InferenceError> {
        let certificate = &root.certificate;
        let size = certificate.substitutions.len() + certificate.effect_substitutions.len() + certificate.effect_roots.len() + certificate.requirement_origins.len();
        self.work_many(size + 1)?;
        if proof.instances.contains(root) { return Ok(()); }
        let scope = PublicationScope::Lexical(root.scope); self.prepare_scope(scope, proof)?;
        for ty in certificate.substitutions.iter().copied().chain(std::iter::once(certificate.signature)) { self.publish_root(ty, scope, proof)?; }
        for effect in certificate.effect_substitutions.iter().copied().map(EffectSummary::Variable).chain(certificate.effect_roots.iter().copied()) { self.validate_effect(effect, scope, proof)?; }
        let substitutions = self.validate_scheme_substitutions(certificate.scheme, certificate.signature, &certificate.substitutions, &certificate.effect_substitutions, &certificate.effect_roots)?;
        let scheme = self.scheme(certificate.scheme)?;
        if scheme.requirements.len() != certificate.requirement_origins.len() || scheme.requirement_origins.len() != certificate.requirement_origins.len() { return Err(InferenceError::InvalidScheme); }
        for (index, (source, instance)) in certificate.requirement_origins.iter().copied().enumerate() {
            self.work()?;
            let scheme = self.scheme(certificate.scheme)?;
            if scheme.requirement_origins[index] != source || self.requirement_source(instance)? != source || self.requirement_origin(instance)? != self.requirement_origin(source)? { return Err(InferenceError::InvalidScheme); }
            let expected = scheme.requirements[index]; let actual = self.requirement(instance)?.template;
            if !self.same_published_requirement(expected, actual, &substitutions)? { return Err(InferenceError::InvalidScheme); }
            self.publish_requirement(ScopedRequirementRoot { requirement: instance, scope: root.scope }, proof)?;
        }
        self.work_many(size + 1)?; proof.instances.insert(root.clone()); Ok(())
    }
    fn same_published_requirement(&mut self, expected: RequirementTemplate, actual: RequirementTemplate, substitutions: &PublishedSubstitutions) -> Result<bool, InferenceError> {
        self.work()?; let mut types = Vec::new(); let mut effects = Vec::new();
        match (expected, actual) {
            (RequirementTemplate::ErrorJoin {join:a},RequirementTemplate::ErrorJoin {join:b}) => {
                if self.error_join(a)?.inputs.len()!=self.error_join(b)?.inputs.len() {return Ok(false)}
                self.work_many(self.error_join(a)?.inputs.len()+2)?;
                let first=self.error_join(a)?;let second=self.error_join(b)?;
                types.extend(first.inputs.iter().copied().zip(second.inputs.iter().copied()));types.push((first.result,second.result));
                match (first.bound,second.bound) {(Some(a),Some(b))=>types.push((a,b)),(None,None)=>{},_=>return Ok(false)}
            },
            (RequirementTemplate::Add { left: a, right: b, result: c }, RequirementTemplate::Add { left: d, right: e, result: f }) => types.extend([(a, d), (b, e), (c, f)]),
            (RequirementTemplate::Eligibility { predicate: a, ty: b }, RequirementTemplate::Eligibility { predicate: c, ty: d }) if a == c => types.push((b, d)),
            (RequirementTemplate::EqualityCompatible { left: a, right: b }, RequirementTemplate::EqualityCompatible { left: c, right: d }) => types.extend([(a, c), (b, d)]),
            (RequirementTemplate::EffectInclusion { actual: a, expected: b, excluded: mask }, RequirementTemplate::EffectInclusion { actual: c, expected: d, excluded: other }) if mask == other => effects.extend([(a, c), (b, d)]),
            (RequirementTemplate::CallableInvocation { call: a }, RequirementTemplate::CallableInvocation { call: b }) => {
                let a = self.invocation_call(a)?; let b = self.invocation_call(b)?;
                if a.domain != b.domain || a.arguments.len() != b.arguments.len() { return Ok(false); }
                let count = a.arguments.len(); self.work_many(count + 3)?;
                let (a, b) = match (expected, actual) { (RequirementTemplate::CallableInvocation { call: a }, RequirementTemplate::CallableInvocation { call: b }) => (self.invocation_call(a)?, self.invocation_call(b)?), _ => unreachable!() };
                types.extend([(a.callable, b.callable), (a.result, b.result)]); effects.push((a.effects, b.effects));
                for (a, b) in a.arguments.iter().zip(&b.arguments) { if a.kind != b.kind { return Ok(false); } types.push((a.ty, b.ty)); }
            }
            (RequirementTemplate::Operation { family: a_family, call: a }, RequirementTemplate::Operation { family: b_family, call: b }) if a_family == b_family => {
                let first = self.operation_call(a)?; let second = self.operation_call(b)?;
                if first.arguments.len() != second.arguments.len() || first.effect_bindings.len() != second.effect_bindings.len() || first.output_effect_bindings.len() != second.output_effect_bindings.len() { return Ok(false); }
                let count = first.arguments.len() + first.effect_bindings.len() + first.output_effect_bindings.len(); self.work_many(count + 3)?;
                let a = self.operation_call(a)?; let b = self.operation_call(b)?;
                match (a.receiver, b.receiver) { (Some(a), Some(b)) => types.push((a, b)), (None, None) => {}, _ => return Ok(false) }
                match (a.declared_error_bound, b.declared_error_bound) { (Some(a), Some(b)) => types.push((a, b)), (None, None) => {}, _ => return Ok(false) }
                for (a, b) in a.arguments.iter().zip(&b.arguments) { match (a, b) { (Some(a), Some(b)) => types.push((*a, *b)), (None, None) => {}, _ => return Ok(false) } }
                types.push((a.result, b.result)); effects.push((a.effects, b.effects));
                for ((a_role, a), (b_role, b)) in a.effect_bindings.iter().zip(&b.effect_bindings) { if a_role != b_role { return Ok(false); } effects.push((*a, *b)); }
                for ((a_role, a), (b_role, b)) in a.output_effect_bindings.iter().zip(&b.output_effect_bindings) { if a_role != b_role { return Ok(false); } effects.push((*a, *b)); }
            }
            _ => return Ok(false),
        }
        self.work_many(types.len() + effects.len())?;
        for (expected, actual) in types { if !self.same_published_type_substituted(expected, actual, Some(substitutions))? { return Ok(false); } }
        for (expected, actual) in effects { if self.published_substituted_effect(expected, true, Some(substitutions))? != self.resolved_effect_summary(actual)? { return Ok(false); } }
        Ok(true)
    }
    fn publish_requirement(&mut self, root: ScopedRequirementRoot, proof: &mut PublicationProof) -> Result<(), InferenceError> {
        let scope = PublicationScope::Lexical(root.scope);
        let mut pending = vec![root.requirement];
        while let Some(id) = pending.pop() {
            self.work()?;
            if !proof.requirements.insert((id, scope)) { continue; }
            self.prepare_scope(scope, proof)?;
            let requirement = self.requirement(id)?;
            self.requirement(requirement.origin)?;
            self.requirement(requirement.source)?;
            let template = requirement.template;
            let evidence_size = requirement.candidate.as_ref().map_or(0, |evidence| evidence.substitutions.len() + evidence.effect_substitutions.len() + evidence.effect_roots.len() + evidence.dependencies.len() + evidence.callback_invocations.len() + 3)
                + requirement.native_children.capacity() * size_of::<NativeInvocationAlternative>()
        + requirement.invocation.as_ref().map_or(0, |evidence| invocation_plan_work(&evidence.plan) + 3);
            self.work_many(evidence_size)?;
            let evidence = self.requirement(id)?.candidate.clone();
            let invocation = self.requirement(id)?.invocation.clone();
            let types = self.requirement_types(template)?;
            let effects = self.requirement_effects(template)?;
            self.work_many(types.len() + effects.len())?;
            for ty in types { self.publish_root(ty, scope, proof)?; }
            for effect in effects { self.validate_effect(effect, scope, proof)?; }
            if let RequirementTemplate::ErrorJoin {join} = template {self.publish_error_join(id,join)?;}
            if let Some(evidence) = evidence {
                let RequirementTemplate::Operation { family, call } = template else { return Err(InferenceError::InvalidScheme); };
                self.work_many(self.family(family)?.len())?;
                if !self.family(family)?.contains(&evidence.candidate) { return Err(InferenceError::InvalidScheme); }
                let call_id = call;
                let candidate = self.candidate(evidence.candidate)?;
                let scheme = candidate.scheme; let has_receiver = candidate.has_receiver; let failure_projection = candidate.failure_projection;
                let TypeNode::Arrow(signature) = self.node(self.resolved(evidence.signature)?)? else { return Err(InferenceError::InvalidScheme); };
                let TypeNode::Arrow(declared) = self.node(self.resolved(self.scheme(candidate.scheme)?.body)?)? else { return Err(InferenceError::InvalidScheme); };
                let declared_kind=declared.kind;
                let signature_params=signature.params.len(); let signature_kind=signature.kind; let signature_effects=signature.effects; let signature_result=signature.result;
                let (call,binding)=self.prepare_candidate_call(evidence.candidate,evidence.signature,call_id)?;
                if binding!=evidence.binding || call.arguments!=evidence.actual_arguments { return Err(InferenceError::InvalidScheme); }
                if signature_kind != declared_kind || signature_params != call.arguments.len() + usize::from(has_receiver)
                    || has_receiver != call.receiver.is_some() || self.resolved(evidence.result)? != self.resolved(call.result)?
                    || self.resolved_effect_summary(signature_effects)? != self.resolved_effect_summary(evidence.effects)? { return Err(InferenceError::InvalidScheme); }
                let failure = match (failure_projection, call.declared_error_bound) {
                    (Some(projection), Some(bound)) => Some((projection, bound)),
                    _ => None,
                };
                if !self.same_published_type(signature_result, evidence.result)? { return Err(InferenceError::InvalidScheme); }
                for ty in evidence.substitutions.iter().copied().chain([evidence.signature, evidence.result]) { self.publish_root(ty, scope, proof)?; }
                for effect in evidence.effect_substitutions.iter().copied().map(EffectSummary::Variable).chain(evidence.effect_roots.iter().copied()).chain(std::iter::once(evidence.effects)) { self.validate_effect(effect, scope, proof)?; }
                self.validate_scheme_instance(scheme, evidence.signature, &evidence.substitutions, &evidence.effect_substitutions, &evidence.effect_roots)?;
                self.validate_candidate_ports(call_id, &evidence)?;
                self.validate_callback_protocols(&call, &evidence)?;
                match (failure, evidence.failure_assignability) {
                    (None, None) => {},
                    (Some((projection, bound)), Some(index)) => {
                        let error = self.operation_failure_type(projection, has_receiver, evidence.signature)?;
                        self.work()?;
                        let origin = *self.origins.get(index).ok_or(InferenceError::InvalidScheme)?;
                        let ConstraintRelation::Assignable { expected, actual } = origin.relation else { return Err(InferenceError::InvalidScheme); };
                        if origin.reason != self.requirement(id)?.reason || !self.same_published_type(expected, bound)? || !self.same_published_type(actual, error)? { return Err(InferenceError::InvalidScheme); }
                    }
                    _ => return Err(InferenceError::InvalidScheme),
                }
                pending.extend(evidence.dependencies);
            }
            if let Some(evidence) = invocation {
                let Some((selected, binding, supplied_timing)) = evidence.unique_plan() else {
                    self.publish_all_invocation(id, template, &evidence, scope, proof, &mut pending)?;
                    continue;
                };
                self.publish_root(selected, scope, proof)?;
                self.publish_root(evidence.result, scope, proof)?;
                self.validate_effect(evidence.effects, scope, proof)?;
                let TypeNode::Arrow(signature) = self.node(self.resolved(selected)?)? else { return Err(InferenceError::InvalidScheme); };
                let RequirementTemplate::CallableInvocation { call } = template else { return Err(InferenceError::InvalidScheme); };
                let call = self.invocation_call(call)?;
                if self.resolved(evidence.callable)? != self.resolved(call.callable)? || self.resolved(evidence.result)? != self.resolved(call.result)?
                    || self.resolved_effect_summary(evidence.effects)? != self.resolved_effect_summary(call.effects)? || self.resolved_effect_summary(evidence.effects)? != self.resolved_effect_summary(signature.effects)?
                    || !call.domain.admits(signature.kind) { return Err(InferenceError::InvalidScheme); }
                let actual_callable = call.callable;
                let selected_signature = selected;
                let signature_kind = signature.kind;
                let signature_result = signature.result;
                let structural_signature=self.callable_signature(actual_callable)?;
                if matches!(self.node(self.resolved(structural_signature)?)?,TypeNode::CallableChoice(_)) {
                    let signatures=self.callable_signatures(actual_callable)?; self.work_many(signatures.len())?;
                    let mut present=false; for signature in signatures { present |= self.same_published_type(signature,selected_signature)?; } if !present { return Err(InferenceError::InvalidScheme); }
                } else if self.resolved(selected_signature)?!=self.resolved(structural_signature)? {
                    if !matches!(self.node(self.resolved(actual_callable)?)?,TypeNode::NativeCallable(callable) if matches!(callable.alternatives.as_slice(),[CallableAuthority::Native {authority:NativeAuthority::Family(_)}])) {return Err(InferenceError::InvalidScheme);}
                }
                let timing = if signature_kind == CallableKind::Stream { InvocationDefaultTiming::AtPull } else { InvocationDefaultTiming::AtCall };
                if supplied_timing != timing { return Err(InferenceError::InvalidScheme); }
                let argument_count = self.invocation_call(match template { RequirementTemplate::CallableInvocation{call}=>call,_=>unreachable!() })?.arguments.len();
                self.work_many(argument_count)?;
                let kinds = self.invocation_call(match template { RequirementTemplate::CallableInvocation { call } => call, _ => unreachable!() })?.arguments.iter().map(|argument| argument.kind).collect::<Vec<_>>();
                let expected = self.plan_invocation_arguments(selected, &kinds).map_err(|error| match error { InvocationPlanError::Graph(error) => error, InvocationPlanError::Binding(_) => InferenceError::InvalidScheme })?;
                if expected.supplied_slots != binding.supplied_slots || expected.default_slots != binding.default_slots || expected.rest_slot != binding.rest_slot || expected.dynamic != binding.dynamic
                    || !self.same_published_type(signature_result, evidence.result)? { return Err(InferenceError::InvalidScheme); }
                let authorities = match self.clone_node(self.resolved(evidence.callable)?)? { TypeNode::NativeCallable(callable) => callable.alternatives, TypeNode::Arrow(_) => Vec::new(), _ => return Err(InferenceError::InvalidScheme) };
                let native_count = authorities.iter().filter(|authority| matches!(authority, CallableAuthority::Native { .. })).count();
                if native_count != evidence.native_alternatives.len() { return Err(InferenceError::InvalidScheme); }
                let mut seen = FxHashSet::default();
                for alternative in &evidence.native_alternatives {
                    self.work()?;
                    if !seen.insert(alternative.authority) || !authorities.contains(&CallableAuthority::Native { authority: alternative.authority }) || binding.dynamic.is_some() || binding.rest_slot.is_some() { return Err(InferenceError::InvalidScheme); }
                    let RequirementTemplate::Operation { family, call: child_id } = self.requirement(alternative.operation)?.template else { return Err(InferenceError::InvalidScheme) };
                    let child = self.operation_call(child_id)?;
                    let child_binding=child.binding;
                    let authority_family = match alternative.authority { NativeAuthority::Single(contract) => self.native_contract(contract)?.family, NativeAuthority::Family(contract) => self.native_family_contract(contract)?.family };
                    if child.mono_authority != Some(alternative.authority) || family != authority_family || child.receiver.is_some() { return Err(InferenceError::InvalidScheme); }
                    let mut expected_arguments = vec![None; match self.node(self.resolved(selected)?)? {TypeNode::Arrow(arrow)=>arrow.params.len(),_=>return Err(InferenceError::InvalidScheme)}];
                    let RequirementTemplate::CallableInvocation { call } = template else { unreachable!() };
                    for (argument, slot) in self.invocation_call(call)?.arguments.iter().zip(&expected.supplied_slots) { expected_arguments[*slot] = Some(argument.ty); }
                    match child_binding {OperationBinding::Slots=>{if self.operation_call(child_id)?.arguments!=expected_arguments{return Err(InferenceError::InvalidScheme);}},OperationBinding::Invocation(child_call)=>{if child_call!=call{return Err(InferenceError::InvalidScheme);}}}
                    let Some(child_evidence) = self.requirement(alternative.operation)?.candidate.as_ref() else {
                        // A uniform family can prove binding before generic
                        // input domains select a member. Its original guarded
                        // obligation remains declaration-owned until a call.
                        if !matches!(scope, PublicationScope::Lexical(Some(_))) || !self.requirement_is_residual(id)? { return Err(InferenceError::InvalidScheme); }
                        pending.push(alternative.operation); continue;
                    };
                    if child_evidence.actual_arguments!=expected_arguments {return Err(InferenceError::InvalidScheme);}
                    let contract = self.native_authority_member(alternative.authority, child_evidence.candidate)?;
                    let mono_signature = self.native_contract(contract)?.instance.ty;
                    if !self.same_published_type(child_evidence.signature, mono_signature)? { return Err(InferenceError::InvalidScheme); }
                    pending.push(alternative.operation);
                }

            }
        }
        Ok(())
    }
    fn error_join_outcome_matches(&mut self, outcome:error_joins::ErrorJoinOutcome, ty:TypeId) -> Result<bool,InferenceError> {
        match outcome {error_joins::ErrorJoinOutcome::Identity(expected)=>self.same_published_type(expected,ty),error_joins::ErrorJoinOutcome::Atom(expected)=>Ok(self.node(self.resolved(ty)?)?==&TypeNode::Atom(expected))}
    }
    fn error_join_receipt(&mut self, requirement:RequirementId, index:usize, expected:TypeId, outcome:error_joins::ErrorJoinOutcome) -> Result<(),InferenceError> {
        self.work()?;let origin=*self.origins.get(index).ok_or(InferenceError::InvalidScheme)?;
        let ConstraintRelation::Assignable {expected:target,actual}=origin.relation else {return Err(InferenceError::InvalidScheme)};
        if origin.reason!=self.requirement(requirement)?.reason||!self.same_published_type(expected,target)?||!self.error_join_outcome_matches(outcome,actual)? {return Err(InferenceError::InvalidScheme)}
        Ok(())
    }
    fn publish_error_join(&mut self, requirement:RequirementId, join:ErrorJoinId) -> Result<(),InferenceError> {
        let result=self.error_join(join)?.result;let bound=self.error_join(join)?.bound;
        let state=self.requirement(requirement)?;let eligible=state.eligibility;let output_receipt=state.error_join_output_assignability;let bound_receipt=state.error_join_assignability;
        let Some(outcome)=self.error_join_outcome(join)? else {if eligible||output_receipt.is_some()||bound_receipt.is_some() {return Err(InferenceError::InvalidScheme)}return Ok(());};
        match bound {
            None=>{if !eligible||output_receipt.is_some()||bound_receipt.is_some()||!self.error_join_outcome_matches(outcome,result)? {return Err(InferenceError::InvalidScheme)}},
            Some(bound)=>{
                let computed_pending=match outcome {error_joins::ErrorJoinOutcome::Identity(ty)=>self.error_join_type_pending(ty)?,error_joins::ErrorJoinOutcome::Atom(_)=>false};
                if computed_pending {if eligible||output_receipt.is_some()||bound_receipt.is_some() {return Err(InferenceError::InvalidScheme)}return Ok(());}
                self.error_join_receipt(requirement,output_receipt.ok_or(InferenceError::InvalidScheme)?,result,outcome)?;
                match bound_receipt {Some(index)=>{if !eligible {return Err(InferenceError::InvalidScheme)}self.error_join_receipt(requirement,index,bound,outcome)?;},None=>{if eligible||!self.error_join_type_pending(bound)? {return Err(InferenceError::InvalidScheme)}}}
            },
        }
        Ok(())
    }
    fn publish_all_invocation(&mut self, id:RequirementId, template:RequirementTemplate, evidence:&InvocationEvidence, scope:PublicationScope, proof:&mut PublicationProof, pending:&mut Vec<RequirementId>) -> Result<(),InferenceError> {
        let RequirementTemplate::CallableInvocation { call } = template else { return Err(InferenceError::InvalidScheme) };
        let InvocationPlan::All { branches } = &evidence.plan else { return Err(InferenceError::InvalidScheme) };
        self.work_many(self.invocation_call(call)?.arguments.len() + branches.len())?;
        let original = self.invocation_call(call)?.clone();
        let TypeNode::NativeCallable(callable) = self.clone_node(self.resolved(original.callable)?)? else { return Err(InferenceError::InvalidScheme) };
        if branches.len() < 2 || branches.len() != callable.alternatives.len() || evidence.callable != original.callable || !self.same_published_type(evidence.result,original.result)? || self.resolved_effect_summary(evidence.effects)? != self.resolved_effect_summary(original.effects)? { return Err(InferenceError::InvalidScheme); }
        self.publish_root(evidence.result,scope,proof)?; self.validate_effect(evidence.effects,scope,proof)?;
        let kinds:Vec<_> = original.arguments.iter().map(|argument| argument.kind).collect();
        let mut seen = FxHashSet::default(); let mut natives = FxHashSet::default(); let mut bits = EffectSet::EMPTY; let mut finite = true; let mut unknown = false; let mut branch_kinds=0u8;
        for branch in branches {
            self.work_many(callable.alternatives.len()+1)?;
            if !seen.insert(branch.authority) || !callable.alternatives.contains(&branch.authority) { return Err(InferenceError::InvalidScheme); }
            self.publish_root(branch.signature,scope,proof)?; self.validate_effect(branch.effects,scope,proof)?;
            let TypeNode::Arrow(arrow) = self.clone_node(self.resolved(branch.signature)?)? else { return Err(InferenceError::InvalidScheme) };
            branch_kinds|=Self::callable_kind_bit(arrow.kind);
            if !self.invocation_branch_kind_allowed(call,arrow.kind)? || branch.timing != if arrow.kind == CallableKind::Stream {InvocationDefaultTiming::AtPull}else{InvocationDefaultTiming::AtCall} || !self.same_published_type(arrow.result,original.result)? || self.resolved_effect_summary(branch.effects)? != self.resolved_effect_summary(arrow.effects)? { return Err(InferenceError::InvalidScheme); }
            let binding = self.plan_invocation_arguments(branch.signature,&kinds).map_err(|error| match error {InvocationPlanError::Graph(error)=>error,InvocationPlanError::Binding(_)=>InferenceError::InvalidScheme})?;
            if binding != branch.binding { return Err(InferenceError::InvalidScheme); }
            match branch.authority {
                CallableAuthority::User {signature,origin} => {
                    if !matches!(self.node(self.resolved(origin)?)?,TypeNode::Arrow(_)) || !self.same_published_type(signature,branch.signature)? { return Err(InferenceError::InvalidScheme); }
                },
                CallableAuthority::Native {authority} => {
                    natives.insert(authority);
                    let mut alternatives = evidence.native_alternatives.iter().filter(|alternative| alternative.authority == authority);
                    let alternative = alternatives.next().ok_or(InferenceError::InvalidScheme)?; if alternatives.next().is_some() { return Err(InferenceError::InvalidScheme); }
                    let RequirementTemplate::Operation {family,call:child_id} = self.requirement(alternative.operation)?.template else {return Err(InferenceError::InvalidScheme)};
                    let child = self.operation_call(child_id)?;
                    let expected_family = match authority {NativeAuthority::Single(id)=>self.native_contract(id)?.family,NativeAuthority::Family(id)=>self.native_family_contract(id)?.family};
                    if child.binding != OperationBinding::Invocation(call) || child.effect_mode != OperationEffectMode::ComputedCreation || child.mono_authority != Some(authority) || family != expected_family || child.receiver.is_some() || !child.arguments.is_empty() {return Err(InferenceError::InvalidScheme)}
                    let selected = self.requirement(alternative.operation)?.candidate.as_ref().ok_or(InferenceError::InvalidScheme)?;
                    if !self.requirement(alternative.operation)?.eligibility || selected.binding.as_ref() != Some(&binding) || !self.same_published_type(selected.signature,branch.signature)? {return Err(InferenceError::InvalidScheme)}
                    let selected = self.requirement(alternative.operation)?.candidate.as_ref().unwrap(); let candidate=selected.candidate;
                    let member = self.native_authority_member(authority,candidate)?;
                    if !self.same_published_type(self.native_contract(member)?.instance.ty,branch.signature)? {return Err(InferenceError::InvalidScheme)}
                    pending.push(alternative.operation);
                },
            }
            match self.resolved_effect_summary(branch.effects)? {EffectSummary::Closed(value)=>bits.0|=value.0,EffectSummary::Unknown=>unknown=true,_=>finite=false}
        }
        if !Self::conditional_kind_allowed(original.domain,branch_kinds) {return Err(InferenceError::InvalidScheme)}
        if evidence.native_alternatives.len() != natives.len() {return Err(InferenceError::InvalidScheme)}
        let actual=self.resolved_effect_summary(evidence.effects)?;
        if unknown { if actual!=EffectSummary::Unknown {return Err(InferenceError::InvalidScheme)} }
        else if finite { if actual!=EffectSummary::Closed(bits) {return Err(InferenceError::InvalidScheme)} }
        else {
            for branch in branches {
                let branch_effect=self.resolved_effect_summary(branch.effects)?;
                let mut found=false;
                for index in 0..self.origins.len() {self.work()?;let origin=self.origins[index];if origin.reason==self.requirement(id)?.reason {if let ConstraintRelation::EffectInclusion {actual:source,expected:target}=origin.relation {if self.resolved_effect_summary(source)?==branch_effect && self.resolved_effect_summary(target)?==actual {found=true;break;}}}}
                if !found {return Err(InferenceError::InvalidScheme)}
            }
        }
        Ok(())
    }
    /// Unification shares variables, but distinct immutable constructors can
    /// describe the same solved type. Certificate comparison checks their
    /// structure without adding constraints or authorizing another binder.
    fn validate_callback_protocols(&mut self, call: &OperationCall, evidence: &CandidateEvidence) -> Result<(), InferenceError> {
        let TypeNode::Arrow(signature) = self.clone_node(self.resolved(evidence.signature)?)? else { return Err(InferenceError::InvalidScheme) };
        self.work_many(signature.params.len() + evidence.callback_invocations.len() + evidence.dependencies.len() + self.candidate(evidence.candidate)?.argument_relations.len())?;
        let dependencies: FxHashSet<_> = evidence.dependencies.iter().copied().collect();
        let relations = self.candidate(evidence.candidate)?.argument_relations.clone();
        let actuals: Vec<_> = call.receiver.into_iter().map(Some).chain(call.arguments.iter().copied()).collect();
        let mut receipts = evidence.callback_invocations.iter();
        for (slot, (parameter, actual)) in signature.params.iter().zip(actuals).enumerate() {
            if relations.get(slot) != Some(&ArgumentRelation::InvocationProtocol) || actual.is_none() { continue; }
            let receipt = receipts.next().ok_or(InferenceError::InvalidScheme)?;
            if receipt.slot != slot || !dependencies.contains(&receipt.invocation) { return Err(InferenceError::InvalidScheme); }
            let RequirementTemplate::CallableInvocation { call } = self.requirement(receipt.invocation)?.template else { return Err(InferenceError::InvalidScheme) };
            let TypeNode::Arrow(protocol) = self.clone_node(self.resolved(parameter.ty)?)? else { return Err(InferenceError::InvalidScheme) };
            let invocation = self.invocation_call(call)?;
            if invocation.domain != CallableDomain::Exact(protocol.kind) || self.resolved(invocation.callable)? != self.resolved(actual.unwrap())? || invocation.arguments.len() != protocol.params.len() { return Err(InferenceError::InvalidScheme); }
            self.work_many(protocol.params.len())?;
            let invocation = self.invocation_call(call)?;
            let callable_result = invocation.result; let callable_effects = invocation.effects; let arguments = invocation.arguments.clone();
            for (argument, parameter) in arguments.iter().zip(&protocol.params) {
                if argument.kind != InvocationArgumentKind::Positional || parameter.defaulted || parameter.rest || !self.same_published_type(argument.ty, parameter.ty)? { return Err(InferenceError::InvalidScheme); }
            }
            if !self.same_published_type(callable_result, protocol.result)? || self.resolved_effect_summary(callable_effects)? != self.resolved_effect_summary(protocol.effects)? { return Err(InferenceError::InvalidScheme); }
        }
        if receipts.next().is_some() { return Err(InferenceError::InvalidScheme); }
        Ok(())
    }
    fn validate_candidate_ports(&mut self, call_id: OperationCallId, evidence: &CandidateEvidence) -> Result<(), InferenceError> {
        let (call, binding)=self.prepare_candidate_call(evidence.candidate,evidence.signature,call_id)?;
        if binding!=evidence.binding || call.arguments!=evidence.actual_arguments {return Err(InferenceError::InvalidScheme);}
        let candidate = self.candidate(evidence.candidate)?;
        if candidate.effect_roles.len() != call.effect_bindings.len() || candidate.output_effect_roles.len() != call.output_effect_bindings.len() { return Err(InferenceError::InvalidScheme); }
        self.work_many(candidate.effect_roles.len() + candidate.output_effect_roles.len())?;
        let inputs = self.candidate(evidence.candidate)?.effect_roles.clone(); let outputs = self.candidate(evidence.candidate)?.output_effect_roles.clone();
        for (role, reference) in inputs {
            let actual = call.effect_bindings.iter().find(|(actual, _)| *actual == role).map(|(_, effect)| *effect).ok_or(InferenceError::InvalidScheme)?;
            let expected = match reference { EffectRoleReference::Binder(index) => EffectSummary::Variable(*evidence.effect_substitutions.get(index as usize).ok_or(InferenceError::InvalidScheme)?), EffectRoleReference::Fixed(bits) => EffectSummary::Closed(bits) };
            if self.resolved_effect_summary(actual)? != self.resolved_effect_summary(expected)? { return Err(InferenceError::InvalidScheme); }
        }
        for (role, ordinal) in outputs {
            let actual = call.output_effect_bindings.iter().find(|(actual, _)| *actual == role).map(|(_, effect)| *effect).ok_or(InferenceError::InvalidScheme)?;
            let expected = *evidence.effect_roots.get(ordinal as usize).ok_or(InferenceError::InvalidScheme)?;
            if self.resolved_effect_summary(actual)? != self.resolved_effect_summary(expected)? { return Err(InferenceError::InvalidScheme); }
        }
        if let Some(authority) = self.operation_call(call_id)?.mono_authority {
            let contract = self.native_authority_member(authority, evidence.candidate)?;
            let native = self.native_contract(contract)?;
            if native.candidate != evidence.candidate || native.instance.substitutions.len() != evidence.substitutions.len() || native.instance.effect_substitutions.len() != evidence.effect_substitutions.len() || native.instance.effect_roots.len() != evidence.effect_roots.len() { return Err(InferenceError::InvalidScheme); }
            let count = native.instance.substitutions.len() + native.instance.effect_substitutions.len() + native.instance.effect_roots.len() + native.instance.requirements.len(); self.work_many(count)?;
            let instance = self.native_contract(contract)?.instance.clone();
            if !self.same_published_type(instance.ty, evidence.signature)? { return Err(InferenceError::InvalidScheme); }
            for (a,b) in instance.substitutions.iter().zip(&evidence.substitutions) { if !self.same_published_type(*a,*b)? { return Err(InferenceError::InvalidScheme); } }
            for (a,b) in instance.effect_substitutions.iter().zip(&evidence.effect_substitutions) { if self.resolved_effect_summary(EffectSummary::Variable(*a))? != self.resolved_effect_summary(EffectSummary::Variable(*b))? { return Err(InferenceError::InvalidScheme); } }
            for (a,b) in instance.effect_roots.iter().zip(&evidence.effect_roots) { if self.resolved_effect_summary(*a)? != self.resolved_effect_summary(*b)? { return Err(InferenceError::InvalidScheme); } }
            for requirement in instance.requirements { self.work_many(evidence.dependencies.len())?; if !evidence.dependencies.contains(&requirement) { return Err(InferenceError::InvalidScheme); } }
        }
        Ok(())
    }
    fn publish_native_callable(&mut self, callable: &NativeCallable, scope: PublicationScope, proof: &mut PublicationProof) -> Result<(), InferenceError> {
        if let TypeNode::CallableChoice(signatures) = self.clone_node(self.resolved(callable.signature)?)? {
            if callable.alternatives.is_empty() { return Err(InferenceError::InvalidScheme); }
            let mut expected = Vec::new(); let mut unique = FxHashSet::default();
            for authority in &callable.alternatives {
                self.work()?; if !unique.insert(*authority) { return Err(InferenceError::InvalidScheme); }
                let signature = match *authority {
                    CallableAuthority::User {signature,origin} => { if !matches!(self.node(self.resolved(origin)?)?,TypeNode::Arrow(_)) {return Err(InferenceError::InvalidScheme);} self.publish_root(signature,scope,proof)?; signature },
                    CallableAuthority::Native {authority:NativeAuthority::Single(contract)} => self.publish_native_contract(contract,scope,proof)?,
                    CallableAuthority::Native {authority:NativeAuthority::Family(id)} => {self.native_family_origin(id)?; if proof.native_families.insert((id,scope)) {self.work_many(self.native_family_contract(id)?.members.len())?;let family=self.native_family_contract(id)?.clone();self.validate_native_family_envelope(&family)?;for member in family.members {self.publish_native_contract(member,scope,proof)?;}} self.native_family_contract(id)?.signature},
                };
                let members = self.callable_signatures(signature)?; self.work_many(members.len())?; expected.extend(members);
            }
            if signatures.len() != expected.len() {return Err(InferenceError::InvalidScheme)}
            self.work_many(expected.len())?; let mut matched=vec![false;expected.len()];
            for signature in signatures {let mut found=false;for (index,expected) in expected.iter().enumerate() {self.work()?;if !matched[index] && self.same_published_type(signature,*expected)? {matched[index]=true;found=true;break;}}if !found {return Err(InferenceError::InvalidScheme)}}
            return Ok(());
        }
        let TypeNode::Arrow(common) = self.clone_node(self.resolved(callable.signature)?)? else { return Err(InferenceError::InvalidScheme) };
        if callable.alternatives.is_empty() { return Err(InferenceError::InvalidScheme); }
        let mut unique = FxHashSet::default();
        for authority in &callable.alternatives {
            self.work()?; if !unique.insert(*authority) { return Err(InferenceError::InvalidScheme); }
            let signature = match *authority {
                CallableAuthority::User { signature, .. } => signature,
                CallableAuthority::Native { authority: NativeAuthority::Single(contract) } => self.publish_native_contract(contract, scope, proof)?,
                CallableAuthority::Native { authority: NativeAuthority::Family(id) } => {
                    self.native_family_origin(id)?;
                    if proof.native_families.insert((id, scope)) {
                        self.work_many(self.native_family_contract(id)?.members.len())?;
                        let family = self.native_family_contract(id)?.clone();
                        self.validate_native_family_envelope(&family)?;
                        for member in &family.members { self.publish_native_contract(*member, scope, proof)?; }
                    }
                    self.native_family_contract(id)?.signature
                },
            };
            let TypeNode::Arrow(arrow) = self.clone_node(self.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme) };
            if arrow.kind != common.kind || arrow.params.len() != common.params.len() { return Err(InferenceError::InvalidScheme); }
            for (a, b) in arrow.params.iter().zip(&common.params) {
                self.work()?; if a.label != b.label || a.defaulted != b.defaulted || a.rest != b.rest || !self.same_published_type(a.ty, b.ty)? { return Err(InferenceError::InvalidScheme); }
            }
            if !self.same_published_type(arrow.result, common.result)? || !self.published_effect_included(arrow.effects, common.effects, scope)? { return Err(InferenceError::InvalidScheme); }
        }
        Ok(())
    }
    fn publish_native_contract(&mut self, contract: NativeContractId, scope: PublicationScope, proof: &mut PublicationProof) -> Result<TypeId, InferenceError> {
        self.native_contract_origin(contract)?;
        let native = self.native_contract(contract)?; let candidate = self.candidate(native.candidate)?;
        if native.scheme != candidate.scheme || candidate.has_receiver { return Err(InferenceError::InvalidScheme); }
        let family = native.family; let candidate = native.candidate; let signature = native.instance.ty;
        self.work_many(self.family(family)?.len())?;
        if !self.family(family)?.contains(&candidate) { return Err(InferenceError::InvalidScheme); }
        if proof.native_contracts.insert((contract, scope)) {
            let instance = &self.native_contract(contract)?.instance;
            self.work_many(instance.substitutions.len() + instance.effect_substitutions.len() + instance.effect_roots.len() + instance.requirements.len() + instance.requirement_origins.len())?;
            let native = self.native_contract(contract)?.clone(); let instance = native.instance;
            let substitutions = self.validate_scheme_substitutions(native.scheme, instance.ty, &instance.substitutions, &instance.effect_substitutions, &instance.effect_roots)?;
            let expected = self.scheme(native.scheme)?;
            if instance.requirement_origins.len() != expected.requirements.len() || instance.requirements.len() != expected.requirements.len() { return Err(InferenceError::InvalidScheme); }
            for (index, (source, actual)) in instance.requirement_origins.iter().copied().enumerate() {
                self.work()?;
                if self.scheme(native.scheme)?.requirement_origins[index] != source || instance.requirements[index] != actual || self.requirement_origin(actual)? != self.requirement_origin(source)? { return Err(InferenceError::InvalidScheme); }
                if !self.same_published_requirement(self.scheme(native.scheme)?.requirements[index], self.requirement(actual)?.template, &substitutions)? { return Err(InferenceError::InvalidScheme); }
                self.publish_requirement(ScopedRequirementRoot { requirement: actual, scope: match scope { PublicationScope::Lexical(scope) => scope, PublicationScope::Owned => Some(native.scheme) } }, proof)?;
            }
            for ty in instance.substitutions.iter().copied().chain(std::iter::once(instance.ty)) { self.publish_root(ty, scope, proof)?; }
            for effect in instance.effect_roots.iter().copied().chain(instance.effect_substitutions.iter().copied().map(EffectSummary::Variable)) { self.work()?; self.validate_effect(effect, scope, proof)?; }
        }
        Ok(signature)
    }
    fn validate_native_family_envelope(&mut self, family: &NativeFamilyContract) -> Result<(), InferenceError> {
        self.work_many(family.members.len() + self.family(family.family)?.len())?;
        let canonical = self.family(family.family)?.to_vec();
        if canonical.len() != family.members.len() || canonical.is_empty() { return Err(InferenceError::InvalidScheme); }
        if let TypeNode::CallableChoice(signatures)=self.clone_node(self.resolved(family.signature)?)? {
            if signatures.len()!=family.members.len() {return Err(InferenceError::InvalidScheme);}
            for ((candidate,member),signature) in canonical.iter().copied().zip(&family.members).zip(signatures) {
                self.work()?;let native=self.native_contract(*member)?;if native.candidate!=candidate || native.family!=family.family || self.candidate(candidate)?.has_receiver {return Err(InferenceError::InvalidScheme);}
                let expected=native.instance.ty; if !self.same_published_type(expected,signature)? {return Err(InferenceError::InvalidScheme);}
            } return Ok(());
        }
        let TypeNode::Arrow(common) = self.clone_node(self.resolved(family.signature)?)? else { return Err(InferenceError::InvalidScheme) };
        self.work_many(common.params.len())?;
        let mut defaults = vec![false; common.params.len()]; let mut domains = vec![Vec::<DomainAlternative>::new(); common.params.len()];
        for (candidate, member) in canonical.into_iter().zip(&family.members) {
            self.work()?;
            let native = self.native_contract(*member)?;
            if native.candidate != candidate || native.family != family.family { return Err(InferenceError::InvalidScheme); }
            let signature = native.instance.ty; let candidate = self.candidate(candidate)?;
            if candidate.has_receiver || !candidate.effect_roles.is_empty() || !candidate.output_effect_roles.is_empty() { return Err(InferenceError::InvalidScheme); }
            let TypeNode::Arrow(arrow) = self.clone_node(self.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme) };
            if arrow.kind != common.kind || arrow.params.len() != common.params.len() || self.resolved_effect_summary(arrow.effects)? != self.resolved_effect_summary(common.effects)? || !self.same_published_type(arrow.result, common.result)? { return Err(InferenceError::InvalidScheme); }
            for (index, parameter) in arrow.params.iter().enumerate() {
                self.work()?; if parameter.label != common.params[index].label || parameter.rest || common.params[index].rest { return Err(InferenceError::InvalidScheme); }
                defaults[index] |= parameter.defaulted;
                let relation = self.candidate(self.native_contract(*member)?.candidate)?.argument_relations.get(index).copied().unwrap_or(ArgumentRelation::Assignable);
                let mut present = false;
                for previous in &domains[index] { self.work()?; if previous.relation == relation && self.same_published_type(previous.ty, parameter.ty)? { present = true; break; } }
                if !present { domains[index].push(DomainAlternative { ty: parameter.ty, relation }); }
            }
        }
        for (index, alternatives) in domains.into_iter().enumerate() {
            self.work_many(alternatives.len())?;
            if common.params[index].defaulted != defaults[index] { return Err(InferenceError::InvalidScheme); }
            if alternatives.len() == 1 && matches!(alternatives[0].relation, ArgumentRelation::Assignable | ArgumentRelation::Exact) {
                if !self.same_published_type(alternatives[0].ty, common.params[index].ty)? { return Err(InferenceError::InvalidScheme); }
            } else {
                let TypeNode::FiniteDomain(actuals) = self.clone_node(self.resolved(common.params[index].ty)?)? else { return Err(InferenceError::InvalidScheme) };
                if actuals.len() != alternatives.len() { return Err(InferenceError::InvalidScheme); }
                for (expected, actual) in alternatives.iter().zip(&actuals) { if expected.relation != actual.relation || !self.same_published_type(expected.ty, actual.ty)? { return Err(InferenceError::InvalidScheme); } }
            }
        }
        Ok(())
    }
    fn published_effect_included(&mut self, actual: EffectSummary, expected: EffectSummary, scope: PublicationScope) -> Result<bool, InferenceError> {
        let actual = self.resolved_effect_summary(actual)?; let expected = self.resolved_effect_summary(expected)?;
        if actual == expected || expected == EffectSummary::Unknown { return Ok(true); }
        if let (EffectSummary::Closed(actual), EffectSummary::Closed(expected)) = (actual, expected) { return Ok(actual.0 & !expected.0 == 0); }
        let PublicationScope::Lexical(Some(member)) = scope else { return Ok(false) };
        self.work_many(self.scheme(member)?.effect_inclusions.len())?;
        let edges = self.scheme(member)?.effect_inclusions.clone(); let mut pending = vec![actual]; let mut seen = FxHashSet::default();
        while let Some(summary) = pending.pop() { self.work()?; if summary == expected { return Ok(true); } if !seen.insert(summary) { continue; } self.work_many(edges.len())?; for (from, to) in &edges { if self.resolved_effect_summary(*from)? == summary { pending.push(self.resolved_effect_summary(*to)?); } } }
        Ok(false)
    }
    pub(super) fn same_published_type(&mut self, left: TypeId, right: TypeId) -> Result<bool, InferenceError> {
        self.same_published_type_substituted(left, right, None)
    }
    fn published_nullable_payload(&mut self,mut item:TypeId,mut depth:usize,mut template:bool,substitutions:Option<&PublishedSubstitutions>)->Result<(TypeId,usize,bool),InferenceError> {
        loop {
            self.work()?;if depth>self.limits.structural_depth{return Err(InferenceError::Limit("structural depth"));}
            (item,template)=self.published_substituted_type(item,template,substitutions)?;
            match self.node(item)? {TypeNode::Optional(inner)=>{item=*inner;depth+=1;},_=>return Ok((item,depth,template))}
        }
    }
    fn same_published_type_substituted(&mut self, left: TypeId, right: TypeId, substitutions: Option<&PublishedSubstitutions>) -> Result<bool, InferenceError> {
        let mut pending = vec![(left, right, 0usize, substitutions.is_some())];
        let mut seen = FxHashMap::default();
        while let Some((left, right, depth, template)) = pending.pop() {
            self.work()?;
            if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
            let (left, template) = self.published_substituted_type(left, template, substitutions)?; let right = self.resolved(right)?;
            if left == right { continue; }
            if seen.get(&(left, right, template)).is_some_and(|previous| *previous >= depth) { continue; }
            seen.insert((left, right, template), depth);
            match (self.clone_node(left)?, self.clone_node(right)?) {
                (TypeNode::Atom(a), TypeNode::Atom(b)) if a == b => {},
                (TypeNode::Rigid { scope: a, index: ai, kind: ak }, TypeNode::Rigid { scope: b, index: bi, kind: bk }) if a == b && ai == bi && ak == bk => {},
                (TypeNode::List(a), TypeNode::List(b)) | (TypeNode::Stream(a), TypeNode::Stream(b)) => pending.push((a, b, depth + 1, template)),
                (TypeNode::Optional(a),TypeNode::Optional(b))=>{let(a,ad,template)=self.published_nullable_payload(a,depth+1,template,substitutions)?;let(b,bd,_)=self.published_nullable_payload(b,depth+1,false,None)?;pending.push((a,b,ad.max(bd),template));},
                (TypeNode::Map(a, b), TypeNode::Map(c, d)) | (TypeNode::Result(a, b), TypeNode::Result(c, d)) => { pending.push((a, c, depth + 1, template)); pending.push((b, d, depth + 1, template)); },
                (TypeNode::Arrow(a), TypeNode::Arrow(b)) => {
                    if a.kind != b.kind || a.params.len() != b.params.len() || self.published_substituted_effect(a.effects, template, substitutions)? != self.resolved_effect_summary(b.effects)? { return Ok(false); }
                    for (a, b) in a.params.iter().zip(&b.params) {
                        if a.label != b.label || a.defaulted != b.defaulted || a.rest != b.rest { return Ok(false); }
                        pending.push((a.ty, b.ty, depth + 1, template));
                    }
                    pending.push((a.result, b.result, depth + 1, template));
                },
                (TypeNode::CallableChoice(a),TypeNode::CallableChoice(b))=>{if a.len()!=b.len(){return Ok(false);} self.work_many(a.len())?; for (a,b) in a.iter().zip(&b){pending.push((*a,*b,depth+1,template));}},
                (TypeNode::FiniteDomain(a), TypeNode::FiniteDomain(b)) => {
                    if a.len() != b.len() { return Ok(false); }
                    for (a,b) in a.iter().zip(&b) { self.work()?; if a.relation != b.relation { return Ok(false); } pending.push((a.ty,b.ty,depth + 1,template)); }
                },
                (TypeNode::NativeCallable(a), TypeNode::NativeCallable(b)) => {
                    if a.alternatives.len() != b.alternatives.len() { return Ok(false); }
                    pending.push((a.signature, b.signature, depth + 1, template));
                    for (a, b) in a.alternatives.iter().zip(&b.alternatives) {
                        self.work()?;
                        match (*a, *b) {
                            (CallableAuthority::User { signature: a, .. }, CallableAuthority::User { signature: b, .. }) => pending.push((a, b, depth + 1, template)),
                            (CallableAuthority::Native { authority: NativeAuthority::Single(a) }, CallableAuthority::Native { authority: NativeAuthority::Single(b) }) => {
                                let aid = a; let bid = b; let a = self.native_contract(aid)?; let b = self.native_contract(bid)?;
                                if a.origin != b.origin || a.candidate != b.candidate || a.scheme != b.scheme || a.family != b.family || a.instance.substitutions.len() != b.instance.substitutions.len() || a.instance.effect_substitutions.len() != b.instance.effect_substitutions.len() || a.instance.effect_roots.len() != b.instance.effect_roots.len() { return Ok(false); }
                                let count = a.instance.substitutions.len() + a.instance.effect_substitutions.len() + a.instance.effect_roots.len(); self.work_many(count)?;
                                let a = &self.native_contract(aid)?.instance; let b = &self.native_contract(bid)?.instance;
                                pending.push((a.ty, b.ty, depth + 1, template));
                                for (a, b) in a.substitutions.iter().zip(&b.substitutions) { pending.push((*a, *b, depth + 1, template)); }
                                let effects: Vec<_> = a.effect_substitutions.iter().zip(&b.effect_substitutions).map(|(a,b)| (EffectSummary::Variable(*a), EffectSummary::Variable(*b))).chain(a.effect_roots.iter().copied().zip(b.effect_roots.iter().copied())).collect();
                                for (a,b) in effects { if self.published_substituted_effect(a, template, substitutions)? != self.resolved_effect_summary(b)? { return Ok(false); } }
                            },
                            (CallableAuthority::Native { authority: NativeAuthority::Family(a) }, CallableAuthority::Native { authority: NativeAuthority::Family(b) }) => {
                                let a = self.native_family_contract(a)?; let b = self.native_family_contract(b)?;
                                if a.origin != b.origin || a.family != b.family || a.members.len() != b.members.len() { return Ok(false); }
                                pending.push((a.signature, b.signature, depth + 1, template));
                                let pairs = a.members.iter().copied().zip(b.members.iter().copied()).collect::<Vec<_>>(); self.work_many(pairs.len())?;
                                for (a,b) in pairs {
                                    let a = self.native_contract(a)?; let b = self.native_contract(b)?;
                                    if a.origin != b.origin || a.candidate != b.candidate || a.scheme != b.scheme || a.family != b.family || a.instance.substitutions.len() != b.instance.substitutions.len() || a.instance.effect_substitutions.len() != b.instance.effect_substitutions.len() || a.instance.effect_roots.len() != b.instance.effect_roots.len() { return Ok(false); }
                                    pending.push((a.instance.ty, b.instance.ty, depth + 1, template));
                                    let types = a.instance.substitutions.iter().copied().zip(b.instance.substitutions.iter().copied()).collect::<Vec<_>>();
                                    let effects = a.instance.effect_roots.iter().copied().zip(b.instance.effect_roots.iter().copied()).chain(a.instance.effect_substitutions.iter().copied().map(EffectSummary::Variable).zip(b.instance.effect_substitutions.iter().copied().map(EffectSummary::Variable))).collect::<Vec<_>>(); self.work_many(types.len() + effects.len())?;
                                    for (a,b) in types { pending.push((a,b,depth + 1,template)); }
                                    for (a,b) in effects { if self.published_substituted_effect(a,template,substitutions)? != self.resolved_effect_summary(b)? { return Ok(false); } }
                                }
                            },
                            _ => return Ok(false),
                        }
                    }
                },
                (TypeNode::Module(a), TypeNode::Module(b)) => {
                    if a.len() != b.len() { return Ok(false); }
                    for (a, b) in a.iter().zip(&b) {
                        if a.label != b.label || a.optional != b.optional { return Ok(false); }
                        pending.push((a.ty, b.ty, depth + 1, template));
                    }
                },
                (TypeNode::Record(a), TypeNode::Record(b)) | (TypeNode::Row(a), TypeNode::Row(b)) => {
                    let (a, at) = self.published_row_shape(a, depth + 1, template, substitutions)?;
                    let (b, bt) = self.published_row_shape(b, depth + 1, false, None)?;
                    if a.len() != b.len() { return Ok(false); }
                    for ((label, a), (other, b)) in a.into_iter().zip(b) {
                        if label != other { return Ok(false); }
                        pending.push((a.0, b.0, depth + 1, a.1));
                    }
                    match (at, bt) { (Some(a), Some(b)) => pending.push((a.0, b.0, depth + 1, a.1)), (None, None) => {}, _ => return Ok(false) }
                },
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    fn published_substituted_type(&self, ty: TypeId, template: bool, substitutions: Option<&PublishedSubstitutions>) -> Result<(TypeId, bool), InferenceError> {
        let ty = self.resolved(ty)?;
        if template { if let Some(replacement) = substitutions.and_then(|substitutions| substitutions.types.get(&ty)) { return Ok((self.resolved(*replacement)?, false)); } }
        Ok((ty, template))
    }
    fn published_substituted_effect(&self, effect: EffectSummary, template: bool, substitutions: Option<&PublishedSubstitutions>) -> Result<EffectSummary, InferenceError> {
        let effect = self.resolved_effect_summary(effect)?;
        Ok(if template { *substitutions.and_then(|substitutions| substitutions.effects.get(&effect)).unwrap_or(&effect) } else { effect })
    }
    fn published_row_shape(&mut self, mut row: RowId, mut depth: usize, mut template: bool, substitutions: Option<&PublishedSubstitutions>) -> Result<(std::collections::BTreeMap<Name, (TypeId, bool)>, Option<(TypeId, bool)>), InferenceError> {
        let mut fields = std::collections::BTreeMap::new();
        loop {
            self.work()?;
            if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
            let data = self.clone_row(row)?;
            for field in data.fields {
                if fields.len() >= self.limits.row_labels { return Err(InferenceError::Limit("row labels")); }
                self.work_many((usize::BITS - fields.len().leading_zeros()) as usize + 1)?;
                if fields.insert(field.label, (field.ty, template)).is_some() { return Err(InferenceError::InvalidScheme); }
            }
            let Some(tail) = data.tail else { return Ok((fields, None)); };
            let (tail, tail_template) = self.published_substituted_type(tail, template, substitutions)?;
            match self.node(tail)? { TypeNode::Row(next) => { row = *next; depth += 1; template = tail_template; }, _ => return Ok((fields, Some((tail, tail_template)))) }
        }
    }
    pub fn validate_scheme_instance(&mut self, scheme: SchemeId, signature: TypeId, types: &[TypeId], effects: &[EffectId], roots: &[EffectSummary]) -> Result<(), InferenceError> {
        self.validate_scheme_substitutions(scheme, signature, types, effects, roots).map(|_| ())
    }
    fn validate_scheme_substitutions(&mut self, scheme: SchemeId, signature: TypeId, types: &[TypeId], effects: &[EffectId], roots: &[EffectSummary]) -> Result<PublishedSubstitutions, InferenceError> {
        let descriptor = self.scheme(scheme)?;
        if descriptor.quantifiers.len() != types.len() || descriptor.binders.len() != types.len()
            || descriptor.effect_quantifiers.len() != effects.len() || descriptor.effect_binders.len() != effects.len()
            || descriptor.effect_roots.len() != roots.len() { return Err(InferenceError::InvalidScheme); }
        let body = descriptor.body;
        self.work_many(types.len() + effects.len() + roots.len())?;
        let mut substitutions = PublishedSubstitutions::default();
        for (index, actual) in types.iter().copied().enumerate() {
            let descriptor = self.scheme(scheme)?; let binder = self.resolved(descriptor.binders[index])?; let kind = descriptor.quantifiers[index].kind;
            if self.type_kind(actual)? != kind || self.type_kind(binder)? != kind { return Err(InferenceError::InvalidScheme); }
            let count = descriptor.quantifiers[index].lacks.len(); self.work_many(count)?;
            let lacks = self.scheme(scheme)?.quantifiers[index].lacks.clone();
            if !self.published_row_lacks(actual, &lacks)? { return Err(InferenceError::InvalidScheme); }
            if substitutions.types.insert(binder, self.resolved(actual)?).is_some() { return Err(InferenceError::InvalidScheme); }
        }
        for (index, actual) in effects.iter().copied().enumerate() {
            let binder = self.resolved_effect_summary(self.scheme(scheme)?.effect_binders[index])?;
            let actual = self.resolved_effect_summary(EffectSummary::Variable(actual))?;
            let quantifier = self.scheme(scheme)?.effect_quantifiers[index];
            if !self.published_effect_bounds(actual, quantifier)? { return Err(InferenceError::InvalidScheme); }
            if substitutions.effects.insert(binder, actual).is_some() { return Err(InferenceError::InvalidScheme); }
        }
        if !self.same_published_type_substituted(body, signature, Some(&substitutions))? { return Err(InferenceError::InvalidScheme); }
        for (index, actual) in roots.iter().copied().enumerate() {
            let expected = self.scheme(scheme)?.effect_roots[index];
            if self.published_substituted_effect(expected, true, Some(&substitutions))? != self.resolved_effect_summary(actual)? { return Err(InferenceError::InvalidScheme); }
        }
        Ok(substitutions)
    }
    fn published_row_lacks(&mut self, mut ty: TypeId, labels: &[Name]) -> Result<bool, InferenceError> {
        if labels.is_empty() { return Ok(true); }
        for _ in 0..=self.limits.structural_depth {
            self.work()?; ty = self.resolved(ty)?;
            match self.clone_node(ty)? {
                TypeNode::Row(row) => {
                    let row = self.clone_row(row)?; self.work_many(labels.len().saturating_mul((usize::BITS - row.fields.len().leading_zeros()) as usize + 1))?;
                    if labels.iter().any(|label| row.fields.binary_search_by_key(label, |field| field.label).is_ok()) { return Ok(false); }
                    if let Some(tail) = row.tail { ty = tail; } else { return Ok(true); }
                }
                TypeNode::Rigid { scope, index, kind: VariableKind::Row } => {
                    let count = self.scheme(scope)?.quantifiers.get(index as usize).ok_or(InferenceError::InvalidScheme)?.lacks.len();
                    self.work_many(labels.len().saturating_mul((usize::BITS - count.leading_zeros()) as usize + 1))?;
                    let quantifier = &self.scheme(scope)?.quantifiers[index as usize]; return Ok(labels.iter().all(|label| quantifier.lacks.binary_search(label).is_ok()));
                }
                TypeNode::Meta(meta) if self.meta(meta)?.kind == VariableKind::Row => {
                    let count = self.meta(meta)?.lacks.len(); self.work_many(labels.len().saturating_mul((usize::BITS - count.leading_zeros()) as usize + 1))?;
                    return Ok(labels.iter().all(|label| self.meta(meta).unwrap().lacks.binary_search(label).is_ok()));
                }
                _ => return Ok(false),
            }
        }
        Err(InferenceError::Limit("structural depth"))
    }
    fn published_effect_bounds(&mut self, actual: EffectSummary, quantifier: EffectQuantifier) -> Result<bool, InferenceError> {
        self.work()?;
        let (lower, upper) = match actual {
            EffectSummary::Closed(bits) => (bits, Some(bits)),
            EffectSummary::Variable(id) => { let variable = slot(&self.effects, id.index(), id.generation)?; (variable.bits, variable.upper) },
            EffectSummary::Rigid { scope, index } => { let actual = self.scheme(scope)?.effect_quantifiers.get(index as usize).ok_or(InferenceError::InvalidScheme)?; (actual.lower, actual.upper) },
            EffectSummary::Unknown => return Ok(quantifier.upper.is_none()),
        };
        Ok(lower.contains(quantifier.lower) && quantifier.upper.map_or(true, |expected| expected.contains(upper.unwrap_or(EffectSet(127)))))
    }

    /// The finite inclusion graph has reached its least fixed point before
    /// publication; unquantified variables publish that closed effect set.
    pub fn closed_effect_summary(&self, effect: EffectSummary) -> Result<EffectSummary, InferenceError> {
        match self.resolved_effect_summary(effect)? {
            EffectSummary::Variable(id) => {
                let variable = slot(&self.effects, id.index(), id.generation)?;
                if !variable.rigid_inputs.is_empty() { return Err(InferenceError::Boundary("captured latent effect union is not a closed effect set")); }
                Ok(EffectSummary::Closed(self.effect_value(id)?))
            }
            effect => Ok(effect),
        }
    }
    fn finish_freeze(mut self, mut proof: PublicationProof) -> Result<SolvedGraph, InferenceError> {
        if self.transactions != 0 || !self.trail.is_empty() || !self.queue.is_empty() { return Err(InferenceError::InvalidScheme); }
        for index in 0..self.schemes.len() {
            self.work()?;
            let slot = &self.schemes[index];
            let scope = SchemeId { index: index as u32, generation: slot.generation };
            let body = slot.value.body;
            if slot.value.requirement_origins.len() != slot.value.requirements.len() { return Err(InferenceError::InvalidScheme); }
            let effect_count = slot.value.effect_binders.len() + slot.value.effect_captures.len() + slot.value.effect_roots.len();
            let cost = slot.value.requirements.len() + slot.value.requirement_origins.len() + slot.value.captures.len() + slot.value.effect_inclusions.len() + effect_count;
            self.work_many(cost)?;
            let slot = &self.schemes[index];
            let requirements = slot.value.requirements.clone();
            let origins = slot.value.requirement_origins.clone();
            let captures = slot.value.captures.clone(); let inclusions = slot.value.effect_inclusions.clone();
            let effects: Vec<_> = slot.value.effect_binders.iter().chain(&slot.value.effect_captures).chain(&slot.value.effect_roots).copied().collect();
            for origin in origins { self.work()?; self.requirement(origin)?; }
            let lexical = PublicationScope::Lexical(Some(scope));
            self.prepare_scope(lexical, &mut proof)?;
            self.publish_root(body, lexical, &mut proof)?;
            for capture in captures { self.publish_root(capture, lexical, &mut proof)?; }
            for template in requirements {
                for ty in self.requirement_types(template)? { self.publish_root(ty, lexical, &mut proof)?; }
                let effects = self.requirement_effects(template)?;
                self.work_many(effects.len())?;
                for effect in effects { self.validate_effect(effect, lexical, &mut proof)?; }
            }
            for effect in effects.into_iter().chain(inclusions.into_iter().flat_map(|(actual, expected)| [actual, expected])) { self.work()?; self.validate_effect(effect, lexical, &mut proof)?; }
        }
        for index in 0..self.effects.len() {
            self.work()?;
            let id = EffectId { index: index as u32, generation: self.effects[index].generation };
            let effect = self.closed_effect_summary(EffectSummary::Variable(id))?;
            self.effects[index].value.binding = Some(effect);
        }
        for index in 0..self.nodes.len() {
            self.work()?;
            if let TypeNode::Arrow(arrow) = &self.nodes[index].value {
                let effect = self.closed_effect_summary(arrow.effects)?;
                if let TypeNode::Arrow(arrow) = &mut self.nodes[index].value { arrow.effects = effect; }
            }
        }
        for index in 0..self.native_contracts.len() {
            self.work_many(self.native_contracts[index].value.instance.effect_roots.len())?;
            let roots = self.native_contracts[index].value.instance.effect_roots.iter().map(|summary| self.closed_effect_summary(*summary)).collect::<Result<_, _>>()?;
            self.native_contracts[index].value.instance.effect_roots = roots;
        }
        for index in 0..self.schemes.len() {
            let binders = self.schemes[index].value.effect_binders.iter().map(|effect| self.closed_effect_summary(*effect)).collect::<Result<_, _>>()?;
            let captures = self.schemes[index].value.effect_captures.iter().map(|effect| self.closed_effect_summary(*effect)).collect::<Result<_, _>>()?;
            let roots = self.schemes[index].value.effect_roots.iter().map(|effect| self.closed_effect_summary(*effect)).collect::<Result<_, _>>()?;
            let inclusions = self.schemes[index].value.effect_inclusions.iter().map(|(actual, expected)| Ok((self.closed_effect_summary(*actual)?, self.closed_effect_summary(*expected)?))).collect::<Result<_, InferenceError>>()?;
            let scheme = &mut self.schemes[index].value; scheme.effect_binders = binders; scheme.effect_captures = captures; scheme.effect_roots = roots; scheme.effect_inclusions = inclusions;
        }
        for index in 0..self.operation_calls.len() {
            self.work()?;
            let effects = self.closed_effect_summary(self.operation_calls[index].value.effects)?;
            let role_count = self.operation_calls[index].value.effect_bindings.len() + self.operation_calls[index].value.output_effect_bindings.len();
            self.work_many(role_count)?;
            let bindings = self.operation_calls[index].value.effect_bindings.iter().map(|(role, effect)| Ok((*role, self.closed_effect_summary(*effect)?))).collect::<Result<_, InferenceError>>()?;
            let outputs = self.operation_calls[index].value.output_effect_bindings.iter().map(|(role, effect)| Ok((*role, self.closed_effect_summary(*effect)?))).collect::<Result<_, InferenceError>>()?;
            self.operation_calls[index].value.effects = effects;
            self.operation_calls[index].value.effect_bindings = bindings;
            self.operation_calls[index].value.output_effect_bindings = outputs;
        }
        for index in 0..self.invocation_calls.len() {
            self.work()?;
            let effects = self.closed_effect_summary(self.invocation_calls[index].value.effects)?;
            self.invocation_calls[index].value.effects = effects;
        }
        for index in 0..self.requirements.len() {
            self.work()?;
            if let Some(evidence) = &self.requirements[index].value.candidate {
                let effects = self.closed_effect_summary(evidence.effects)?;
                let root_count = evidence.effect_roots.len();
                self.work_many(root_count)?;
                let roots = self.requirements[index].value.candidate.as_ref().unwrap().effect_roots.iter().map(|effect| self.closed_effect_summary(*effect)).collect::<Result<_, _>>()?;
                if let Some(evidence) = &mut self.requirements[index].value.candidate { evidence.effects = effects; evidence.effect_roots = roots; }
            }
            if let Some(effects) = self.requirements[index].value.invocation.as_ref().map(|evidence| evidence.effects) {
                self.work()?;
                let effects = self.closed_effect_summary(effects)?;
                let branch_effects = match &self.requirements[index].value.invocation.as_ref().unwrap().plan { InvocationPlan::All {branches} => { self.work_many(branches.len())?; match &self.requirements[index].value.invocation.as_ref().unwrap().plan { InvocationPlan::All {branches} => branches.iter().map(|branch|self.closed_effect_summary(branch.effects)).collect::<Result<Vec<_>,_>>()?, _=>unreachable!() } }, _=>Vec::new() };
                if let Some(evidence) = &mut self.requirements[index].value.invocation { evidence.effects = effects; if let InvocationPlan::All {branches}=&mut evidence.plan {for (branch,effect) in branches.iter_mut().zip(branch_effects) {branch.effects=effect;}} }
            }
        }
        Ok(SolvedGraph { context: self, publication: proof })
    }
}

fn invocation_binding_work(binding: &InvocationBinding) -> usize { binding.supplied_slots.len() + binding.default_slots.len() + dynamic_binding_work(binding.dynamic.as_ref()) }
pub(super) fn invocation_plan_work(plan: &InvocationPlan) -> usize { match plan { InvocationPlan::Unique { binding, .. } => invocation_binding_work(binding), InvocationPlan::All { branches } => branches.len() + branches.iter().map(|branch| invocation_binding_work(&branch.binding)).sum::<usize>() } }
fn invocation_binding_bytes(binding: &InvocationBinding) -> usize { (binding.supplied_slots.capacity() + binding.default_slots.capacity()) * size_of::<usize>() + dynamic_binding_bytes(binding.dynamic.as_ref()) }
fn invocation_plan_bytes(plan: &InvocationPlan) -> usize { match plan { InvocationPlan::Unique { binding, .. } => invocation_binding_bytes(binding), InvocationPlan::All { branches } => branches.capacity() * size_of::<InvocationBranchEvidence>() + branches.iter().map(|branch| invocation_binding_bytes(&branch.binding)).sum::<usize>() } }

fn dynamic_binding_work(binding: Option<&DynamicInvocationBinding>) -> usize {
    binding.map_or(0, |binding| binding.conditional_default_slots.len() + binding.required_slots.len() + binding.segments.len() + binding.segments.iter().map(|segment| match segment { InvocationArgumentSegment::DynamicRange { fixed_slots, .. } => fixed_slots.len(), _ => 0 }).sum::<usize>())
}
fn dynamic_binding_bytes(binding: Option<&DynamicInvocationBinding>) -> usize {
    binding.map_or(0, |binding| (binding.conditional_default_slots.capacity() + binding.required_slots.capacity()) * size_of::<usize>() + binding.segments.capacity() * size_of::<InvocationArgumentSegment>() + binding.segments.iter().map(|segment| match segment { InvocationArgumentSegment::DynamicRange { fixed_slots, .. } => fixed_slots.capacity() * size_of::<usize>(), _ => 0 }).sum::<usize>())
}

fn meta_owned_bytes(meta: &Meta) -> usize { meta.lacks.capacity() * size_of::<Name>() + meta.watchers.capacity() * size_of::<RequirementId>() }
fn effect_owned_bytes(effect: &EffectVariable) -> usize { (effect.outgoing.capacity() + effect.incoming.capacity()) * size_of::<EffectId>() + effect.watchers.capacity() * size_of::<RequirementId>() + effect.rigid_inputs.capacity() * size_of::<EffectSummary>() }
fn instance_certificate_bytes(certificate: &InstanceCertificate) -> usize {
    certificate.substitutions.capacity() * size_of::<TypeId>() + certificate.effect_substitutions.capacity() * size_of::<EffectId>()
        + certificate.effect_roots.capacity() * size_of::<EffectSummary>() + certificate.requirement_origins.capacity() * size_of::<(RequirementId, RequirementId)>()
}
fn requirement_owned_bytes(requirement: &Requirement) -> usize {
    requirement.candidate.as_ref().map_or(0, |evidence| evidence.substitutions.capacity() * size_of::<TypeId>() + evidence.actual_arguments.capacity() * size_of::<Option<TypeId>>() + evidence.binding.as_ref().map_or(0, |binding| (binding.supplied_slots.capacity() + binding.default_slots.capacity()) * size_of::<usize>() + dynamic_binding_bytes(binding.dynamic.as_ref())) + evidence.effect_substitutions.capacity() * size_of::<EffectId>() + evidence.effect_roots.capacity() * size_of::<EffectSummary>() + evidence.dependencies.capacity() * size_of::<RequirementId>() + evidence.callback_invocations.capacity() * size_of::<CallableProtocolEvidence>())
        + requirement.native_children.capacity() * size_of::<NativeInvocationAlternative>()
        + requirement.invocation.as_ref().map_or(0, |evidence| invocation_plan_bytes(&evidence.plan) + evidence.native_alternatives.capacity() * size_of::<NativeInvocationAlternative>())
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    use crate::source::SourceId;

    #[test]
    fn publication_enforces_its_work_budget() {
        let mut graph = InferenceContext::default();
        let int = graph.atom(Atom::Int).unwrap();
        graph.limits.work_units = graph.counters.work_units;
        assert!(matches!(graph.freeze_scoped(&[ScopedRoot { ty: int, scope: None }]), Err(InferenceError::Limit("solver work"))));
    }

    #[test]
    fn shared_subgraphs_do_not_hide_a_deeper_path() {
        let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
        let mut graph = InferenceContext::new(Limits { structural_depth: 3, ..Limits::default() });
        let int = graph.atom(Atom::Int).unwrap();
        let leaf = graph.list(int).unwrap();
        let shared = graph.list(leaf).unwrap();
        let prefix = graph.list(shared).unwrap();
        let deep = graph.list(prefix).unwrap();
        let root = graph.module(vec![ModuleField { label: Name::intern("deep"), ty: deep, optional: false }, ModuleField { label: Name::intern("shallow"), ty: shared, optional: false }]).unwrap();
        assert!(matches!(graph.freeze_scoped(&[ScopedRoot { ty: root, scope: None }]), Err(InferenceError::Limit("structural depth"))));
    }

    #[test]
    fn frozen_scheme_effect_endpoints_and_variable_bindings_are_closed() {
        let mut graph = InferenceContext::default();
        let int = graph.atom(Atom::Int).unwrap();
        let effect = graph.fresh_effect_at(0, None).unwrap();
        let why = graph.reason(Span::new(SourceId::new(0), 0, 1), None).unwrap();
        graph.include_effects(EffectSummary::Closed(EffectSet::ENV), EffectSummary::Variable(effect), why).unwrap();
        let scheme = graph.generalize(int, 0, Generalization::Allowed, &[]).unwrap();
        graph.schemes[scheme.index()].value.effect_inclusions.push((EffectSummary::Variable(effect), EffectSummary::Unknown));
        graph.schemes[scheme.index()].value.effect_roots.push(EffectSummary::Variable(effect));
        let call = OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: Vec::new(), result: int, effects: EffectSummary::Variable(effect), effect_bindings: vec![(EffectRole::Pull { source: 0 }, EffectSummary::Variable(effect)), (EffectRole::Close { source: 0 }, EffectSummary::Unknown)], output_effect_bindings: vec![(ProducerRole::Pull, EffectSummary::Variable(effect)), (ProducerRole::Close, EffectSummary::Unknown)] };
        graph.operation_calls.push(Slot { generation: InferenceContext::generation().unwrap(), value: call });
        let solved = graph.freeze_scoped(&[ScopedRoot { ty: int, scope: Some(scheme) }]).unwrap();
        assert_eq!(solved.scheme(scheme).unwrap().effect_inclusions[0].0, EffectSummary::Closed(EffectSet::ENV));
        assert_eq!(solved.scheme(scheme).unwrap().effect_roots, vec![EffectSummary::Closed(EffectSet::ENV)]);
        assert_eq!(solved.operation_calls[0].value.effects, EffectSummary::Closed(EffectSet::ENV));
        assert_eq!(solved.operation_calls[0].value.effect_bindings, vec![(EffectRole::Pull { source: 0 }, EffectSummary::Closed(EffectSet::ENV)), (EffectRole::Close { source: 0 }, EffectSummary::Unknown)]);
        assert_eq!(solved.operation_calls[0].value.output_effect_bindings, vec![(ProducerRole::Pull, EffectSummary::Closed(EffectSet::ENV)), (ProducerRole::Close, EffectSummary::Unknown)]);
        assert_eq!(solved.resolved_effect_summary(EffectSummary::Variable(effect)).unwrap(), EffectSummary::Closed(EffectSet::ENV));
    }
    #[test]
    fn shared_publication_roots_charge_only_one_graph_walk() {
        fn publication_cost(root_count: usize) -> u64 {
            let mut graph = InferenceContext::default();
            let mut ty = graph.atom(Atom::Int).unwrap();
            for _ in 0..32 { ty = graph.list(ty).unwrap(); }
            let before = graph.counters.work_units;
            let solved = graph.freeze_scoped(&vec![ScopedRoot { ty, scope: None }; root_count]).unwrap();
            let after = solved.counters.work_units;
            solved.validate_scoped(ScopedRoot { ty, scope: None }).unwrap();
            assert_eq!(solved.counters.work_units, after);
            assert!(solved.retained_storage().validation_capacity > 0);
            after - before
        }
        assert_eq!(publication_cost(200), publication_cost(1) + 199);
    }

    #[test]
    fn external_effect_roots_reject_a_sibling_scheme_binder() {
        let mut graph = InferenceContext::default();
        let unit = graph.atom(Atom::Unit).unwrap();
        let mut schemes = Vec::new();
        for _ in 0..2 {
            let effect = graph.fresh_effect(None).unwrap();
            let arrow = graph.arrow(Arrow { kind: CallableKind::Proc, params: Vec::new(), result: unit, effects: EffectSummary::Variable(effect) }).unwrap();
            schemes.push(graph.generalize(arrow, 0, Generalization::Allowed, &[]).unwrap());
        }
        let foreign = graph.scheme(schemes[0]).unwrap().effect_binders[0];
        let roots = [ScopedEffectRoot { effect: foreign, scope: Some(schemes[1]) }];
        assert!(matches!(graph.freeze_scoped_with_effects(&[], &roots), Err(InferenceError::ScopeEscape)));
    }

    #[test]
    fn source_requirement_roots_validate_the_exact_member_scope() {
        fn frozen_source(use_sibling: bool) -> Result<SolvedGraph, InferenceError> {
            let mut graph = InferenceContext::default();
            let span = Span::new(SourceId::new(0), 0, 1);
            let mut schemes = Vec::new();
            for _ in 0..2 {
                let ty = graph.fresh(1, span)?;
                schemes.push(graph.generalize(ty, 0, Generalization::Allowed, &[])?);
            }
            let binder = graph.scheme(schemes[0])?.binders[0];
            let int = graph.atom(Atom::Int)?;
            let why = graph.reason(span, None)?;
            let requirement = graph.require_add(binder, int, binder, why)?;
            let scope = Some(schemes[usize::from(use_sibling)]);
            let root = ScopedRequirementRoot { requirement, scope };
            let solved = graph.freeze_scoped_with_facts(&[], &[], &[root])?;
            solved.validate_requirement_scoped(root)?;
            let sibling = ScopedRequirementRoot { requirement, scope: Some(schemes[1]) };
            assert!(matches!(solved.validate_requirement_scoped(sibling), Err(InferenceError::Boundary(_))));
            Ok(solved)
        }
        assert!(frozen_source(false).is_ok());
        assert!(matches!(frozen_source(true), Err(InferenceError::ScopeEscape)));
    }

    #[test]
    fn candidate_output_roots_do_not_authorize_foreign_effect_binders() {
        let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
        let mut graph = InferenceContext::default();
        let int = graph.atom(Atom::Int).unwrap();
        let mut scopes = Vec::new();
        for _ in 0..2 {
            let effect = graph.fresh_effect(None).unwrap();
            let arrow = graph.arrow(Arrow { kind: CallableKind::Proc, params: Vec::new(), result: int, effects: EffectSummary::Variable(effect) }).unwrap();
            scopes.push(graph.generalize(arrow, 0, Generalization::Allowed, &[]).unwrap());
        }
        let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: Vec::new(), result: int, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
        let name = Name::intern("publication-output-scope");
        let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: name, public_label: name, effect_roles: Vec::new(), output_effect_roles: Vec::new(), scheme, has_receiver: false, actual_eligibility: Vec::new(), argument_relations: Vec::new() }).unwrap();
        let family = graph.register_family(&[candidate]).unwrap();
        let why = graph.reason(Span::new(SourceId::new(0), 0, 1), None).unwrap();
        let call = OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: Vec::new(), result: int, effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: Vec::new(), output_effect_bindings: Vec::new() };
        let requirement = graph.require_operation(family, call, why).unwrap();
        graph.solve().unwrap();
        let foreign = graph.scheme(scopes[0]).unwrap().effect_binders[0];
        graph.requirements[requirement.index()].value.candidate.as_mut().unwrap().effect_roots.push(foreign);
        let roots = [ScopedRequirementRoot { requirement, scope: Some(scopes[1]) }];
        assert!(matches!(graph.freeze_scoped_with_facts(&[], &[], &roots), Err(InferenceError::ScopeEscape)));
    }

    #[test]
    fn repeated_source_requirement_roots_share_the_certificate_walk() {
        fn publication_cost(count: usize) -> u64 {
            let mut graph = InferenceContext::default();
            let int = graph.atom(Atom::Int).unwrap();
            let why = graph.reason(Span::new(SourceId::new(0), 0, 1), None).unwrap();
            let requirement = graph.require_add(int, int, int, why).unwrap();
            graph.solve().unwrap();
            let root = ScopedRequirementRoot { requirement, scope: None };
            let before = graph.counters.work_units;
            let solved = graph.freeze_scoped_with_facts(&[], &[], &vec![root; count]).unwrap();
            let after = solved.counters.work_units;
            solved.validate_requirement_scoped(root).unwrap();
            assert_eq!(solved.counters.work_units, after);
            after - before
        }
        assert_eq!(publication_cost(200), publication_cost(1) + 199);
    }

    #[test]
    fn publication_refuses_a_candidate_signature_that_is_not_its_call_certificate() {
        let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
        for non_callable in [true, false] {
            let mut graph = InferenceContext::default();
            let int = graph.atom(Atom::Int).unwrap();
            let boolean = graph.atom(Atom::Bool).unwrap();
            let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: Vec::new(), result: int, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
            let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
            let name = Name::intern("signature-certificate");
            let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: name, public_label: name, effect_roles: Vec::new(), output_effect_roles: Vec::new(), scheme, has_receiver: false, actual_eligibility: Vec::new(), argument_relations: Vec::new() }).unwrap();
            let family = graph.register_family(&[candidate]).unwrap();
            let why = graph.reason(Span::new(SourceId::new(0), 0, 1), None).unwrap();
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: Vec::new(), result: int, effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: Vec::new(), output_effect_bindings: Vec::new() }, why).unwrap();
            graph.solve().unwrap();
            let wrong = if non_callable { boolean } else { graph.arrow(Arrow { kind: CallableKind::Pure, params: Vec::new(), result: boolean, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap() };
            graph.requirements[requirement.index()].value.candidate.as_mut().unwrap().signature = wrong;
            assert!(matches!(graph.freeze_scoped_with_facts(&[], &[], &[ScopedRequirementRoot { requirement, scope: None }]), Err(InferenceError::InvalidScheme)));
        }
    }

    #[test]
    fn candidate_signature_publication_checks_each_declared_parameter_property() {
        fn publish(alter: u8) -> Result<SolvedGraph, InferenceError> {
            let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
            let mut graph = InferenceContext::default(); let int = graph.atom(Atom::Int)?; let boolean = graph.atom(Atom::Bool)?; let list = graph.list(int)?;
            let mut arrow = Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("values"), ty: list, defaulted: false, rest: false }], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY) };
            let signature = graph.arrow(arrow.clone())?; let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[])?;
            let name = Name::intern("parameter-certificate");
            let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: name, public_label: name, scheme, has_receiver: false, effect_roles: Vec::new(), output_effect_roles: Vec::new(), actual_eligibility: Vec::new(), argument_relations: Vec::new() })?;
            let family = graph.register_family(&[candidate])?; let why = graph.reason(Span::at(SourceId::new(0), 0), None)?;
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(list)], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: Vec::new(), output_effect_bindings: Vec::new() }, why)?;
            graph.solve()?;
            match alter { 1 => arrow.params[0].ty = boolean, 2 => arrow.params[0].label = Name::intern("other"), 3 => arrow.params[0].defaulted = true, 4 => arrow.params[0].rest = true, _ => {} }
            let signature = graph.arrow(arrow)?;
            graph.requirements[requirement.index()].value.candidate.as_mut().unwrap().signature = signature;
            graph.freeze_scoped_with_facts(&[], &[], &[ScopedRequirementRoot { requirement, scope: None }])
        }
        assert!(publish(0).is_ok());
        for alteration in 1..=4 { assert!(matches!(publish(alteration), Err(InferenceError::InvalidScheme)), "alteration {alteration}"); }
    }

    #[test]
    fn native_instance_receipts_preserve_all_substitutions_and_immediate_origins() {
        let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
        let mut graph = InferenceContext::default(); let span = Span::at(SourceId::new(0), 0); let why = graph.reason(span, None).unwrap();
        let ty = graph.fresh(1, span).unwrap(); let effect = graph.fresh_effect_at(1, None).unwrap();
        let requirement = graph.require_add(ty, ty, ty, why).unwrap();
        let signature = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("value"), ty, defaulted: false, rest: false }], result: ty, effects: EffectSummary::Variable(effect) }).unwrap();
        let scheme = graph.generalize_with_effect_roots(signature, 0, Generalization::Allowed, &[requirement], &[EffectSummary::Variable(effect)]).unwrap();
        let instance = graph.instantiate(scheme, 0, why).unwrap(); let int = graph.atom(Atom::Int).unwrap(); let boolean = graph.atom(Atom::Bool).unwrap();
        graph.unify(instance.substitutions[0], int, why).unwrap(); graph.equate_effects(EffectSummary::Variable(instance.effect_substitutions[0]), EffectSummary::Closed(EffectSet::TIME), why).unwrap(); graph.solve().unwrap();
        let root = ScopedInstanceRoot { scope: None, certificate: InstanceCertificate { scheme, signature: instance.ty, substitutions: instance.substitutions, effect_substitutions: instance.effect_substitutions, effect_roots: instance.effect_roots, requirement_origins: instance.requirement_origins } };
        let before = graph.counters().clone(); let nodes = graph.nodes.len(); let variables = graph.metas.len() + graph.effects.len();
        let solved = graph.freeze_scoped_with_instances(&[], &[], &[], &[root.clone(), root.clone()]).unwrap();
        assert_eq!(solved.publication.instances.len(), 1); assert_eq!(solved.context.nodes.len(), nodes); assert_eq!(solved.context.metas.len() + solved.context.effects.len(), variables);
        assert_eq!(solved.counters().instantiations, before.instantiations); assert_eq!(solved.counters().attempted_constraints, before.attempted_constraints);
        assert!(solved.retained_storage().validation_bytes >= instance_certificate_bytes(&root.certificate));
        let published_work = solved.counters().work_units; solved.validate_instance_scoped(&root).unwrap(); assert_eq!(solved.counters().work_units, published_work);
        for alteration in 0..7 {
            let mut changed = root.clone();
            match alteration {
                0 => changed.certificate.signature = signature,
                1 => changed.certificate.substitutions[0] = boolean,
                2 => changed.certificate.effect_substitutions.clear(),
                3 => changed.certificate.effect_roots[0] = EffectSummary::Closed(EffectSet::ENV),
                4 => changed.certificate.requirement_origins[0].0 = changed.certificate.requirement_origins[0].1,
                5 => changed.certificate.scheme = SchemeId { index: scheme.index, generation: scheme.generation + 1 },
                _ => changed.scope = Some(scheme),
            }
            assert!(solved.validate_instance_scoped(&changed).is_err(), "alteration {alteration}");
        }
    }

    #[test]
    fn candidate_instances_preserve_open_rows_effect_roots_and_declared_erasure() {
        let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
        let mut graph = InferenceContext::default(); let span = Span::at(SourceId::new(0), 0); let why = graph.reason(span, None).unwrap();
        let item = graph.fresh(1, span).unwrap(); let tail = graph.fresh_row(1, span).unwrap();
        let row = graph.row(vec![RowField { label: Name::intern("name"), ty: item }], Some(tail)).unwrap(); let record = graph.record(row).unwrap();
        let effect = graph.fresh_effect_at(1, None).unwrap();
        let signature = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("entry"), ty: record, defaulted: false, rest: false }], result: item, effects: EffectSummary::Variable(effect) }).unwrap();
        let scheme = graph.generalize_with_effect_roots(signature, 0, Generalization::Allowed, &[], &[EffectSummary::Variable(effect)]).unwrap();
        let instance = graph.instantiate(scheme, 0, why).unwrap(); let int = graph.atom(Atom::Int).unwrap(); let boolean = graph.atom(Atom::Bool).unwrap();
        let row = graph.row(vec![RowField { label: Name::intern("name"), ty: int }, RowField { label: Name::intern("active"), ty: boolean }], None).unwrap(); let actual = graph.record(row).unwrap();
        let TypeNode::Arrow(arrow) = graph.node(instance.ty).unwrap() else { panic!() }; let parameter = arrow.params[0].ty;
        graph.unify(parameter, actual, why).unwrap(); graph.equate_effects(instance.effect_roots[0], EffectSummary::Closed(EffectSet::TIME), why).unwrap();
        graph.validate_scheme_instance(scheme, instance.ty, &instance.substitutions, &instance.effect_substitutions, &instance.effect_roots).unwrap();
        let mut wrong = instance.effect_roots.clone(); wrong[0] = EffectSummary::Closed(EffectSet::ENV);
        assert!(matches!(graph.validate_scheme_instance(scheme, instance.ty, &instance.substitutions, &instance.effect_substitutions, &wrong), Err(InferenceError::InvalidScheme)));
        assert!(matches!(graph.validate_scheme_instance(scheme, instance.ty, &instance.substitutions[..1], &instance.effect_substitutions, &instance.effect_roots), Err(InferenceError::InvalidScheme)));
        let any = graph.atom(Atom::Any).unwrap(); let declared = graph.list(any).unwrap(); let actual = graph.list(int).unwrap();
        let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("values"), ty: declared, defaulted: false, rest: false }], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap(); let name = Name::intern("declared-input-erasure");
        let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: name, public_label: name, scheme, has_receiver: false, effect_roles: Vec::new(), output_effect_roles: Vec::new(), actual_eligibility: Vec::new(), argument_relations: vec![ArgumentRelation::DeclaredErasure] }).unwrap();
        let family = graph.register_family(&[candidate]).unwrap(); let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(actual)], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: Vec::new(), output_effect_bindings: Vec::new() }, why).unwrap(); graph.solve().unwrap();
        graph.freeze_scoped_with_facts(&[], &[], &[ScopedRequirementRoot { requirement, scope: None }]).unwrap();
    }

    #[test]
    fn cached_alias_suffix_preserves_the_substitution_depth_guard() {
        let mut graph = InferenceContext::new(Limits { structural_depth: 3, ..Limits::default() });
        let int = graph.atom(Atom::Int).unwrap();
        let span = Span::new(SourceId::new(0), 0, 1);
        let variables: Vec<_> = (0..5).map(|_| graph.fresh(0, span).unwrap()).collect();
        for (index, ty) in variables.iter().enumerate() {
            let TypeNode::Meta(id) = *graph.node(*ty).unwrap() else { panic!() };
            graph.metas[id.index()].value.binding = Some(variables.get(index + 1).copied().unwrap_or(int));
        }
        let roots = [ScopedRoot { ty: variables[3], scope: None }, ScopedRoot { ty: variables[0], scope: None }];
        assert!(matches!(graph.freeze_scoped(&roots), Err(InferenceError::Limit("substitution depth"))));
    }

    #[test]
    fn source_invocation_publication_rejects_altered_binding_and_timing_evidence() {
        fn publish(alter: u8) -> Result<SolvedGraph, InferenceError> {
            let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
            let mut graph = InferenceContext::default();
            let int = graph.atom(Atom::Int)?;
            let parameter = |name| Parameter { label: Name::intern(name), ty: int, defaulted: true, rest: false };
            let signature = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![parameter("first"), parameter("second"), parameter("chosen")], result: int, effects: EffectSummary::Closed(EffectSet::TIME) })?;
            let why = graph.reason(Span::at(SourceId::new(0), 0), None)?;
            let requirement = graph.require_callable_invocation(InvocationCall { callable: signature, arguments: vec![InvocationArgument { kind: InvocationArgumentKind::Named(Name::intern("chosen")), ty: int }], result: int, effects: EffectSummary::Closed(EffectSet::TIME), domain: CallableDomain::AnyCallable }, why)?;
            graph.solve()?;
            let evidence = graph.requirements[requirement.index()].value.invocation.as_mut().unwrap();
            let InvocationPlan::Unique { binding, timing, .. } = &mut evidence.plan else { unreachable!() };
            match alter {
                1 => binding.supplied_slots[0] = 0,
                2 => binding.default_slots.reverse(),
                3 => *timing = InvocationDefaultTiming::AtPull,
                4 => evidence.effects = EffectSummary::Closed(EffectSet::ENV),
                _ => {},
            }
            graph.freeze_scoped_with_facts(&[], &[], &[ScopedRequirementRoot { requirement, scope: None }])
        }
        assert!(publish(0).is_ok());
        for alteration in 1..=4 { assert!(matches!(publish(alteration), Err(InferenceError::InvalidScheme)), "alteration {alteration}"); }
    }

    #[test]
    fn dynamic_invocation_publication_replays_the_exact_structural_binding() {
        fn publish(alter: u8) -> Result<SolvedGraph, InferenceError> {
            let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
            let mut graph = InferenceContext::default();
            let int = graph.atom(Atom::Int)?;
            let list = graph.list(int)?;
            let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![
                Parameter { label: Name::intern("first"), ty: int, defaulted: false, rest: false },
                Parameter { label: Name::intern("second"), ty: int, defaulted: true, rest: false },
            ], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY) })?;
            let reason = graph.reason(Span::at(SourceId::new(0), 0), None)?;
            let requirement = graph.require_callable_invocation(InvocationCall { callable: signature,
                arguments: vec![InvocationArgument { kind: InvocationArgumentKind::PositionalSplice, ty: list }],
                result: int, effects: EffectSummary::Closed(EffectSet::EMPTY), domain: CallableDomain::Pure }, reason)?;
            graph.solve()?;
            let evidence = graph.requirements[requirement.index()].value.invocation.as_mut().unwrap();
            let InvocationPlan::Unique { binding, .. } = &mut evidence.plan else { unreachable!() };
            let dynamic = binding.dynamic.as_mut().unwrap();
            match alter {
                1 => dynamic.required_slots.clear(),
                2 => dynamic.conditional_default_slots.clear(),
                3 => dynamic.runtime_arity_guard = false,
                4 => dynamic.runtime_duplicate_guard = false,
                5 => dynamic.segments.clear(),
                6 => binding.supplied_slots.push(0),
                _ => {},
            }
            graph.freeze_scoped_with_facts(&[], &[], &[ScopedRequirementRoot { requirement, scope: None }])
        }
        assert!(publish(0).is_ok());
        for alteration in 1..=6 { assert!(matches!(publish(alteration), Err(InferenceError::InvalidScheme)), "alteration {alteration}"); }
    }

    #[test]
    fn requirement_origin_handles_are_provenance_without_scope_authority() {
        let mut foreign = InferenceContext::default();
        let int = foreign.atom(Atom::Int).unwrap();
        let why = foreign.reason(Span::at(SourceId::new(0), 0), None).unwrap();
        let foreign_id = foreign.require_add(int, int, int, why).unwrap();
        let mut graph = InferenceContext::default();
        let int = graph.atom(Atom::Int).unwrap();
        let why = graph.reason(Span::at(SourceId::new(0), 0), None).unwrap();
        let id = graph.require_add(int, int, int, why).unwrap();
        graph.solve().unwrap();
        graph.requirements[id.index()].value.source = foreign_id;
        assert!(matches!(graph.freeze_scoped_with_facts(&[], &[], &[ScopedRequirementRoot { requirement: id, scope: None }]), Err(InferenceError::ForeignHandle)));
    }

    #[test]
    fn candidate_failure_receipts_preserve_direction_scope_and_reason() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
        fn publish(alteration: usize) -> Result<SolvedGraph, InferenceError> {
            let mut graph = InferenceContext::default(); let span = Span::at(SourceId::new(0), 0);
            let why = graph.reason(span, None)?; let other_reason = graph.reason(span, None)?;
            let int = graph.atom(Atom::Int)?; let error = graph.fresh(1, span)?;
            let allowed = graph.atom(Atom::ErrorFamily(Name::intern("SourceFailure")))?;
            let variant = graph.atom(Atom::ErrorVariant { family: Name::intern("SourceFailure"), variant: Name::intern("Failed") })?;
            let unrelated = graph.atom(Atom::ErrorFamily(Name::intern("OtherFailure")))?;
            let parameter = graph.result(int, error)?;
            let signature = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("values"), ty: parameter, defaulted: false, rest: false }], result: int, effects: EffectSummary::Closed(EffectSet::ERROR) })?;
            let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[])?;
            let candidate = graph.register_candidate(CandidateTemplate { failure_projection: Some(OperationFailureProjection::ArgumentResultError { argument: 0 }), identity: Name::intern("failure-receipt"), public_label: Name::intern("iterate"), effect_roles: vec![], output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![ArgumentRelation::Assignable] })?;
            let family = graph.register_family(&[candidate])?; let actual = graph.result(int, variant)?;
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: Some(allowed), receiver: None, arguments: vec![Some(actual)], result: int, effects: EffectSummary::Closed(EffectSet::ERROR), effect_bindings: vec![], output_effect_bindings: vec![] }, why)?; graph.solve()?;
            let correct = graph.candidate_evidence(requirement)?.unwrap().failure_assignability.unwrap();
            let equality = graph.origins.len(); graph.unify(int, int, why)?;
            let wrong_reason = graph.origins.len(); graph.assignable(allowed, variant, other_reason)?;
            let wrong_direction = graph.origins.len(); graph.assignable(variant, variant, why)?;
            let evidence = graph.requirements[requirement.index()].value.candidate.as_mut().unwrap();
            match alteration {
                1 => evidence.failure_assignability = None,
                2 => evidence.failure_assignability = Some(equality),
                3 => evidence.failure_assignability = Some(wrong_reason),
                4 => evidence.failure_assignability = Some(wrong_direction),
                5 => { let RequirementTemplate::Operation { call, .. } = graph.requirement_template(requirement)? else { panic!() }; graph.operation_calls[call.index()].value.declared_error_bound = Some(unrelated); },
                _ => assert_eq!(evidence.failure_assignability, Some(correct)),
            }
            graph.freeze_scoped_with_facts(&[], &[], &[ScopedRequirementRoot { requirement, scope: None }])
        }
        assert!(publish(0).is_ok());
        for alteration in 1..=5 { assert!(publish(alteration).is_err(), "alteration {alteration}"); }
    }

}

#[cfg(test)]
mod error_join_publication_tests {
    use super::*;
    use crate::source::SourceId;

    #[test]
    fn error_join_publication_replays_calculated_result_and_directional_bound_receipt() {
        fn publish(alter:u8,widened:bool) -> Result<SolvedGraph,InferenceError> {
            let symbols=crate::symbol::SymbolOwner::new();let _symbols=symbols.enter();let mut graph=InferenceContext::default();
            let left=graph.atom(Atom::ErrorVariant {family:Name::intern("FirstFailure"),variant:Name::intern("Missing")})?;
            let right=graph.atom(Atom::ErrorVariant {family:Name::intern("FirstFailure"),variant:Name::intern("Denied")})?;
            let bound=graph.atom(Atom::Error)?;let result=if widened {bound}else{graph.fresh(0,Span::at(SourceId::new(0),0))?};let reason=graph.reason(Span::at(SourceId::new(0),0),None)?;
            let requirement=graph.require_error_join(ErrorJoin {inputs:vec![left,right],result,bound:Some(bound)},reason)?;graph.solve()?;
            let RequirementTemplate::ErrorJoin {join}=graph.requirement_template(requirement)? else {unreachable!()};
            match alter {
                1=>{let string=graph.atom(Atom::Str)?;graph.error_joins[join.index()].value.result=string;},
                2=>{graph.requirements[requirement.index()].value.error_join_assignability=None;},
                3=>{let index=graph.requirement(requirement)?.error_join_assignability.unwrap();let ConstraintRelation::Assignable {expected,actual}=graph.origins[index].relation else {unreachable!()};graph.origins[index].relation=ConstraintRelation::Assignable {expected:actual,actual:expected};},
                4=>{let index=graph.requirement(requirement)?.error_join_assignability.unwrap();let other=graph.reason(Span::at(SourceId::new(0),1),None)?;graph.origins[index].reason=other;},
                5=>{let foreign=graph.atom(Atom::ErrorFamily(Name::intern("OtherFailure")))?;graph.error_joins[join.index()].value.inputs[0]=foreign;},
                6=>{graph.requirements[requirement.index()].value.error_join_output_assignability=None;},
                7=>{let index=graph.requirement(requirement)?.error_join_output_assignability.unwrap();graph.origins[index].relation=ConstraintRelation::Assignable {expected:result,actual:bound};},
                _=>{},
            }
            graph.freeze_scoped_with_facts(&[],&[],&[ScopedRequirementRoot {requirement,scope:None}])
        }
        for widened in [false,true] {
            assert!(publish(0,widened).is_ok());for alter in 1..=7 {assert!(matches!(publish(alter,widened),Err(InferenceError::InvalidScheme)),"alteration {alter}, widened {widened}");}
        }
    }

    #[test]
    fn independent_symbolic_error_inputs_remain_scoped_residual_publication_obligations() {
        fn publish(complete:bool) -> Result<SolvedGraph,InferenceError> {
            let symbols=crate::symbol::SymbolOwner::new();let _symbols=symbols.enter();let mut graph=InferenceContext::default();let span=Span::at(SourceId::new(0),0);let reason=graph.reason(span,None)?;
            let left=graph.fresh(1,span)?;let right=graph.fresh(1,span)?;let result=graph.fresh(1,span)?;
            let requirement=graph.require_error_join(ErrorJoin {inputs:vec![left,right],result,bound:None},reason)?;
            let arrow=graph.arrow(Arrow {kind:CallableKind::Pure,params:vec![Parameter {label:Name::intern("left"),ty:left,defaulted:false,rest:false},Parameter {label:Name::intern("right"),ty:right,defaulted:false,rest:false}],result,effects:EffectSummary::Closed(EffectSet::EMPTY)})?;
            let scheme=graph.generalize(arrow,0,Generalization::Allowed,&[requirement])?;assert_ne!(graph.resolved(left)?,graph.resolved(right)?);
            graph.requirements[requirement.index()].value.eligibility=complete;
            graph.freeze_scoped_with_facts(&[ScopedRoot {ty:arrow,scope:Some(scheme)}],&[],&[ScopedRequirementRoot {requirement,scope:Some(scheme)}])
        }
        assert!(publish(false).is_ok());assert!(matches!(publish(true),Err(InferenceError::InvalidScheme)));
    }
}
