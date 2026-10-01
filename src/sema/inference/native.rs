use super::*;

impl NativeContract {
    pub fn certificate(&self) -> InstanceCertificate {
        InstanceCertificate { scheme: self.scheme, signature: self.instance.ty, substitutions: self.instance.substitutions.clone(), effect_substitutions: self.instance.effect_substitutions.clone(), effect_roots: self.instance.effect_roots.clone(), requirement_origins: self.instance.requirement_origins.clone() }
    }
}

impl InferenceContext {
    pub fn native_family_contract(&self, id: NativeFamilyContractId) -> Result<&NativeFamilyContract, InferenceError> { slot(&self.native_families, id.index(), id.generation) }
    pub fn native_family_origin(&self, id: NativeFamilyContractId) -> Result<NativeFamilyContractId, InferenceError> {
        let origin = self.native_family_contract(id)?.origin;
        if self.native_family_contract(origin)?.origin != origin { return Err(InferenceError::InvalidScheme); } Ok(origin)
    }
    pub fn native_family_member(&self, id: NativeFamilyContractId, candidate: CandidateId) -> Result<NativeContractId, InferenceError> {
        self.candidate(candidate)?;
        for member in &self.native_family_contract(id)?.members { if self.native_contract(*member)?.candidate == candidate { return Ok(*member); } }
        Err(InferenceError::InvalidScheme)
    }
    pub fn native_authority_signature(&self, authority: NativeAuthority) -> Result<TypeId, InferenceError> { match authority { NativeAuthority::Single(id) => Ok(self.native_contract(id)?.instance.ty), NativeAuthority::Family(id) => Ok(self.native_family_contract(id)?.signature) } }
    pub fn native_authority_origin(&self, authority: NativeAuthority) -> Result<NativeAuthority, InferenceError> { match authority { NativeAuthority::Single(id) => Ok(NativeAuthority::Single(self.native_contract_origin(id)?)), NativeAuthority::Family(id) => Ok(NativeAuthority::Family(self.native_family_origin(id)?)) } }
    pub fn native_authority_member(&self, authority: NativeAuthority, candidate: CandidateId) -> Result<NativeContractId, InferenceError> { match authority { NativeAuthority::Single(id) if self.native_contract(id)?.candidate == candidate => Ok(id), NativeAuthority::Family(id) => self.native_family_member(id, candidate), _ => Err(InferenceError::KindMismatch) } }
    pub fn native_family_callable(&mut self, family: OperationFamilyId, instances: Vec<(CandidateId, Instantiation)>, reason: ReasonId) -> Result<TypeId, InferenceError> {
        self.probe(|graph| {
            graph.reason_data(reason)?; graph.work_many(graph.family(family)?.len() + instances.len())?;
            let candidates = graph.family(family)?.to_vec();
            if candidates.len() != instances.len() { return Err(InferenceError::InvalidScheme); }
            let mut supplied = FxHashMap::default();
            for (candidate, instance) in instances { graph.work()?; if supplied.insert(candidate, instance).is_some() { return Err(InferenceError::InvalidScheme); } }
            let mut members = Vec::with_capacity(candidates.len());
            for candidate in candidates {
                let instance = supplied.remove(&candidate).ok_or(InferenceError::InvalidScheme)?;
                let template = graph.candidate(candidate)?;
                if template.has_receiver || !template.effect_roles.is_empty() { return Err(InferenceError::Boundary("native family needs explicit receiver or producer input roles")); }
                for ty in std::iter::once(instance.ty).chain(instance.substitutions.iter().copied()) {
                    if !graph.free_metas(ty)?.is_empty() || !graph.rigid_nodes(ty)?.is_empty() || !graph.free_effects(ty)?.is_empty() || !graph.rigid_effects(ty)?.is_empty() { return Err(InferenceError::Boundary("native family member must have a complete monotype")); }
                }
                members.push(graph.make_native_contract(candidate, family, instance)?);
            }
            let signature = graph.native_family_signature(&members)?;
            let placeholder = NativeFamilyContractId { index: 0, generation: 0 };
            let contract = graph.store_native_family(NativeFamilyContract { origin: placeholder, family, signature, members }, true)?;
            graph.allocate(TypeNode::NativeCallable(NativeCallable { signature, alternatives: vec![CallableAuthority::Native { authority: NativeAuthority::Family(contract) }] }))
        })
    }
    fn store_native_family(&mut self, mut contract: NativeFamilyContract, original: bool) -> Result<NativeFamilyContractId, InferenceError> {
        self.work_many(contract.members.len() + 1)?; self.constraint()?;
        let id = NativeFamilyContractId { index: self.native_families.len().try_into().map_err(|_| InferenceError::Limit("native families"))?, generation: Self::generation()? };
        if original { contract.origin = id; } else { self.native_family_origin(contract.origin)?; }
        self.native_families.push(Slot { generation: id.generation, value: contract }); Ok(id)
    }
    fn native_family_signature(&mut self, members: &[NativeContractId]) -> Result<TypeId, InferenceError> {
        self.work_many(members.len())?;
        let first = *members.first().ok_or(InferenceError::InvalidScheme)?;
        let TypeNode::Arrow(common) = self.clone_node(self.resolved(self.native_contract(first)?.instance.ty)?)? else { return Err(InferenceError::InvalidScheme) };
        let mut uniform = true; let mut signatures = Vec::with_capacity(members.len());
        for member in members {
            let contract = self.native_contract(*member)?; let signature = contract.instance.ty; let candidate = contract.candidate;
            let TypeNode::Arrow(arrow) = self.clone_node(self.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme) };
            self.work_many(arrow.params.len())?;
            uniform &= self.candidate(candidate)?.output_effect_roles.is_empty() && arrow.kind == common.kind && arrow.params.len() == common.params.len() && arrow.params.iter().zip(&common.params).all(|(a,b)| a.label == b.label && a.rest == b.rest && !a.rest) && self.resolved_effect_summary(arrow.effects)? == self.resolved_effect_summary(common.effects)? && self.same_published_type(arrow.result,common.result)?;
            signatures.push(signature);
        }
        if uniform { let arrow = self.native_family_envelope(members)?; self.arrow(arrow) } else { self.allocate(TypeNode::CallableChoice(signatures)) }
    }
    /// The public parameter domains describe possible inputs. The complete
    /// member signatures retain correlations between slots and exact defaults.
    pub(super) fn native_family_envelope(&mut self, members: &[NativeContractId]) -> Result<Arrow, InferenceError> {
        let first = *members.first().ok_or(InferenceError::InvalidScheme)?;
        let TypeNode::Arrow(mut common) = self.clone_node(self.resolved(self.native_contract(first)?.instance.ty)?)? else { return Err(InferenceError::InvalidScheme) };
        self.work_many(common.params.len() + members.len())?;
        let mut domains = vec![Vec::<DomainAlternative>::new(); common.params.len()];
        for member in members {
            let contract = self.native_contract(*member)?; let signature = contract.instance.ty; let candidate = contract.candidate;
            let TypeNode::Arrow(arrow) = self.clone_node(self.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme) };
            if arrow.kind != common.kind || arrow.params.len() != common.params.len() || self.resolved_effect_summary(arrow.effects)? != self.resolved_effect_summary(common.effects)? || !self.same_published_type(arrow.result, common.result)? { return Err(InferenceError::Boundary("native family members need one result and effect protocol")); }
            for (index, parameter) in arrow.params.iter().enumerate() {
                self.work()?;
                if parameter.label != common.params[index].label || parameter.rest != common.params[index].rest || parameter.rest { return Err(InferenceError::Boundary("native family members need one fixed binding shape")); }
                common.params[index].defaulted |= parameter.defaulted;
                let relation = self.candidate(candidate)?.argument_relations.get(index).copied().unwrap_or(ArgumentRelation::Assignable);
                let alternative = DomainAlternative { ty: parameter.ty, relation };
                let mut present = false;
                for previous in &domains[index] { self.work()?; if previous.relation == relation && self.same_published_type(previous.ty, parameter.ty)? { present = true; break; } }
                if !present { domains[index].push(alternative); }
            }
        }
        for (parameter, alternatives) in common.params.iter_mut().zip(domains) {
            parameter.ty = if alternatives.len() == 1 && matches!(alternatives[0].relation, ArgumentRelation::Assignable | ArgumentRelation::Exact) { alternatives[0].ty } else { self.allocate(TypeNode::FiniteDomain(alternatives))? };
        }
        Ok(common)
    }
    pub(super) fn replace_native_authority(&mut self, authority: NativeAuthority, types: &FxHashMap<TypeId, TypeId>, effects: &FxHashMap<EffectSummary, EffectSummary>, memo: &mut ReplacementMemo, depth: usize) -> Result<NativeAuthority, InferenceError> {
        match authority {
            NativeAuthority::Single(id) => Ok(NativeAuthority::Single(self.replace_native_contract(id, types, effects, memo, depth)?)),
            NativeAuthority::Family(id) => {
                self.work()?; if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
                if let Some(replacement) = memo.families.get(&id) { return Ok(NativeAuthority::Family(*replacement)); }
                self.work_many(self.native_family_contract(id)?.members.len())?;
                let mut family = self.native_family_contract(id)?.clone();
                family.signature = self.replace(family.signature, types, effects, memo, depth + 1)?;
                for member in &mut family.members { *member = self.replace_native_contract(*member, types, effects, memo, depth + 1)?; }
                let replacement = self.store_native_family(family, false)?;
                memo.families.insert(id, replacement); Ok(NativeAuthority::Family(replacement))
            },
        }
    }
    pub fn native_contract(&self, id: NativeContractId) -> Result<&NativeContract, InferenceError> { slot(&self.native_contracts, id.index(), id.generation) }
    pub fn native_contract_origin(&self, id: NativeContractId) -> Result<NativeContractId, InferenceError> {
        let origin = self.native_contract(id)?.origin;
        if self.native_contract(origin)?.origin != origin { return Err(InferenceError::InvalidScheme); }
        Ok(origin)
    }
    pub fn callable_signature(&self, ty: TypeId) -> Result<TypeId, InferenceError> {
        let ty = self.resolved(ty)?;
        match self.node(ty)? {
            TypeNode::Arrow(_) | TypeNode::CallableChoice(_) => Ok(ty),
            TypeNode::NativeCallable(callable) if matches!(self.node(self.resolved(callable.signature)?)?, TypeNode::Arrow(_) | TypeNode::CallableChoice(_)) => Ok(callable.signature),
            _ => Err(InferenceError::KindMismatch),
        }
    }
    pub fn callable_signatures(&self, ty: TypeId) -> Result<Vec<TypeId>, InferenceError> {
        let signature = self.callable_signature(ty)?;
        match self.node(self.resolved(signature)?)? {
            TypeNode::Arrow(_) => Ok(vec![signature]),
            TypeNode::CallableChoice(signatures) => { for signature in signatures { if !matches!(self.node(self.resolved(*signature)?)?, TypeNode::Arrow(_)) { return Err(InferenceError::InvalidScheme); } } Ok(signatures.clone()) },
            _ => Err(InferenceError::InvalidScheme),
        }
    }
    pub(super) fn type_effect_roots(&self, ty: TypeId) -> Result<Vec<EffectSummary>, InferenceError> {
        let mut roots = Vec::new();
        match self.node(ty)? {
            TypeNode::Arrow(arrow) => roots.push(arrow.effects),
            TypeNode::NativeCallable(callable) => for authority in &callable.alternatives {
                if let CallableAuthority::Native { authority } = authority {
                    let members = match authority { NativeAuthority::Single(contract) => vec![*contract], NativeAuthority::Family(family) => self.native_family_contract(*family)?.members.clone() };
                    for contract in members { let instance = &self.native_contract(contract)?.instance;
                    roots.extend_from_slice(&instance.effect_roots);
                    roots.extend(instance.effect_substitutions.iter().copied().map(EffectSummary::Variable));
                    for requirement in &instance.requirements { roots.extend(self.requirement_effects(self.requirement(*requirement)?.template)?); }
                    }
                }
            },
            _ => {},
        }
        Ok(roots)
    }
    fn store_native_contract(&mut self, mut contract: NativeContract, original: bool) -> Result<NativeContractId, InferenceError> {
        let instance = &contract.instance;
        self.work_many(instance.requirement_origins.len() + instance.effect_roots.len() + instance.requirements.len() + instance.substitutions.len() + instance.effect_substitutions.len() + 1)?;
        self.constraint()?;
        let id = NativeContractId { index: self.native_contracts.len().try_into().map_err(|_| InferenceError::Limit("native contracts"))?, generation: Self::generation()? };
        if original { contract.origin = id; } else { self.native_contract_origin(contract.origin)?; }
        self.native_contracts.push(Slot { generation: id.generation, value: contract }); Ok(id)
    }
    pub(super) fn replace_native_contract(&mut self, id: NativeContractId, types: &FxHashMap<TypeId, TypeId>, effects: &FxHashMap<EffectSummary, EffectSummary>, memo: &mut ReplacementMemo, depth: usize) -> Result<NativeContractId, InferenceError> {
        self.work()?; if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
        if let Some(replacement) = memo.contracts.get(&id) { return Ok(*replacement); }
        let original = self.native_contract(id)?;
        let instance = &original.instance;
        self.work_many(instance.requirement_origins.len() + instance.effect_roots.len() + instance.requirements.len() + instance.substitutions.len() + instance.effect_substitutions.len())?;
        let mut contract = self.native_contract(id)?.clone();
        contract.instance.ty = self.replace(contract.instance.ty, types, effects, memo, depth + 1)?;
        for ty in &mut contract.instance.substitutions { *ty = self.replace(*ty, types, effects, memo, depth + 1)?; }
        for summary in &mut contract.instance.effect_roots { let resolved = self.resolved_effect_summary(*summary)?; *summary = *effects.get(&resolved).unwrap_or(&resolved); }
        for effect in &mut contract.instance.effect_substitutions {
            let resolved = self.resolved_effect_summary(EffectSummary::Variable(*effect))?;
            let replacement = *effects.get(&resolved).unwrap_or(&resolved);
            *effect = match replacement {
                EffectSummary::Variable(id) => id,
                _ if replacement == resolved => *effect,
                _ => { let variable = slot(&self.effects, effect.index(), effect.generation)?; let level = variable.level; let upper = variable.upper; let id = self.fresh_effect_at(level, upper)?; self.unify_effects(EffectSummary::Variable(id), replacement)?; id },
            };
        }
        let mut requirements = Vec::with_capacity(contract.instance.requirements.len());
        for source in contract.instance.requirements.iter().copied() {
            let previous_source = self.requirement(source)?.source;
            let requirement = if let Some(replacement) = memo.requirements.get(&source).or_else(|| if memo.sources.contains(&previous_source) { memo.requirements.get(&previous_source) } else { None }) { *replacement } else {
                let template = self.requirement(source)?.template; let reason = self.requirement(source)?.reason; let origin = self.requirement_origin(source)?;
                let template = self.replace_requirement(template, types, effects, memo)?;
                let requirement = self.instantiate_requirement(template, reason)?;
                self.requirements[requirement.index()].value.source = source; self.requirements[requirement.index()].value.origin = origin;
                memo.requirements.insert(source, requirement);
                if memo.sources.contains(&previous_source) { memo.requirements.insert(previous_source, requirement); }
                self.replace_native_invocation_children(source, requirement, types, effects, memo, depth + 1)?;
                requirement
            };
            requirements.push(requirement);
        }
        for (old, new) in contract.instance.requirements.iter().zip(&requirements) { memo.requirements.insert(*old, *new); }
        contract.instance.requirements = requirements;
        for pair in &mut contract.instance.requirement_origins { pair.1 = *memo.requirements.get(&pair.1).ok_or(InferenceError::InvalidScheme)?; }
        let replacement = self.store_native_contract(contract, false)?;
        memo.contracts.insert(id, replacement); Ok(replacement)
    }
    fn make_native_contract(&mut self, candidate: CandidateId, family: OperationFamilyId, instance: Instantiation) -> Result<NativeContractId, InferenceError> {
        let scheme = self.candidate(candidate)?.scheme;
        self.work_many(self.family(family)?.len())?;
        if !self.family(family)?.contains(&candidate) || self.candidate(candidate)?.has_receiver { return Err(InferenceError::InvalidScheme); }
        self.validate_scheme_instance(scheme, instance.ty, &instance.substitutions, &instance.effect_substitutions, &instance.effect_roots)?;
        if instance.requirements.len() != instance.requirement_origins.len() || instance.requirements.len() != self.scheme(scheme)?.requirements.len() { return Err(InferenceError::InvalidScheme); }
        for (requirement, (source, actual)) in instance.requirements.iter().zip(&instance.requirement_origins) { self.work()?; if requirement != actual || self.requirement(*actual)?.source != *source { return Err(InferenceError::InvalidScheme); } }
        let placeholder = NativeContractId { index: 0, generation: 0 };
        self.store_native_contract(NativeContract { origin: placeholder, candidate, scheme, family, instance }, true)
    }
    pub fn native_callable(&mut self, candidate: CandidateId, family: OperationFamilyId, instance: Instantiation) -> Result<TypeId, InferenceError> {
        self.probe(|graph| {
            let signature = instance.ty;
            let contract = graph.make_native_contract(candidate, family, instance)?;
            graph.allocate(TypeNode::NativeCallable(NativeCallable { signature, alternatives: vec![CallableAuthority::Native { authority: NativeAuthority::Single(contract) }] }))
        })
    }
    pub fn join_callable_values(&mut self, left: TypeId, right: TypeId, level: u32, reason: ReasonId) -> Result<TypeId, InferenceError> {
        self.probe(|graph| {
            let a = graph.callable_signature(left)?; let b = graph.callable_signature(right)?;
            let left_signature = graph.clone_node(graph.resolved(a)?)?; let right_signature = graph.clone_node(graph.resolved(b)?)?;
            let mut common = None;
            if let (TypeNode::Arrow(mut arrow), TypeNode::Arrow(other)) = (left_signature, right_signature) {
                if arrow.kind == other.kind && arrow.params.len() == other.params.len() && arrow.params.iter().zip(&other.params).all(|(a,b)| a.label == b.label && a.defaulted == b.defaulted && a.rest == b.rest) {
                    graph.work_many(arrow.params.len())?;
                    let mut compatible=graph.same_published_type(arrow.result,other.result)?;
                    for (parameter, other) in arrow.params.iter().zip(&other.params) { compatible &= graph.same_published_type(parameter.ty,other.ty)?; }
                    if compatible {
                    let first = graph.resolved_effect_summary(arrow.effects)?; let second = graph.resolved_effect_summary(other.effects)?;
                    arrow.effects = match (first, second) {
                        (EffectSummary::Closed(a), EffectSummary::Closed(b)) => EffectSummary::Closed(EffectSet(a.0 | b.0)),
                        (EffectSummary::Unknown, _) | (_, EffectSummary::Unknown) => EffectSummary::Unknown,
                        _ => { let effect = EffectSummary::Variable(graph.fresh_derived_effect_at(level, None)?); graph.include_effects(first, effect, reason)?; graph.include_effects(second, effect, reason)?; effect },
                    };
                    common = Some(graph.arrow(arrow)?);
                    }
                }
            }
            let mut alternatives = Vec::new();
            for value in [left, right] {
                let authority = match graph.clone_node(graph.resolved(value)?)? { TypeNode::NativeCallable(callable) => callable.alternatives, TypeNode::Arrow(_) => vec![CallableAuthority::User { signature: graph.resolved(value)?, origin: graph.resolved(value)? }], _ => return Err(InferenceError::KindMismatch) };
                for authority in authority { graph.work_many(alternatives.len() + 1)?; if !alternatives.contains(&authority) { alternatives.push(authority); } }
            }
            graph.work_many(alternatives.len().saturating_mul((usize::BITS - alternatives.len().leading_zeros()) as usize))?;
            alternatives.sort_by_key(|authority| match authority { CallableAuthority::User { signature, .. } => (0, signature.generation()), CallableAuthority::Native { authority: NativeAuthority::Single(contract) } => (1, contract.generation()), CallableAuthority::Native { authority: NativeAuthority::Family(contract) } => (2, contract.generation()) });
            let signature = match common { Some(signature) => signature, None => {
                let mut signatures = Vec::new();
                for authority in &alternatives {
                    let signature = match *authority { CallableAuthority::User {signature,..} => signature, CallableAuthority::Native {authority} => graph.native_authority_signature(authority)? };
                    let count = match graph.node(graph.resolved(signature)?)? {TypeNode::Arrow(_)=>1,TypeNode::CallableChoice(members)=>members.len(),_=>return Err(InferenceError::InvalidScheme)};
                    graph.work_many(count)?;
                    signatures.extend(graph.callable_signatures(signature)?);
                }
                graph.allocate(TypeNode::CallableChoice(signatures))?
            } };
            graph.allocate(TypeNode::NativeCallable(NativeCallable { signature, alternatives }))
        })
    }
    pub(super) fn native_invocation_operations(&mut self, requirement: RequirementId, callable: TypeId, arguments: &[InvocationArgument], binding: &InvocationBinding) -> Result<Vec<NativeInvocationAlternative>, InferenceError> {
        let TypeNode::NativeCallable(wrapper) = self.clone_node(self.resolved(callable)?)? else { return Ok(Vec::new()) };
        if binding.dynamic.is_some() || binding.rest_slot.is_some() { return Err(InferenceError::Boundary("native invocation requires a fixed supplied argument plan")); }
        let signature = self.callable_signature(callable)?;
        let TypeNode::Arrow(arrow) = self.clone_node(self.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme) };
        let reason = self.requirement(requirement)?.reason;
        let mut result = Vec::new();
        for alternative in wrapper.alternatives {
            let CallableAuthority::Native { authority } = alternative else { continue };
            self.work_many(arrow.params.len() + arguments.len())?;
            let operation = if let Some(previous) = self.native_invocation_children(requirement)?.iter().find(|alternative| alternative.authority == authority) { previous.operation } else {
                let (family, signature, outputs) = match authority {
                    NativeAuthority::Single(contract) => {
                        let native = self.native_contract(contract)?; let candidate = self.candidate(native.candidate)?;
                        if candidate.has_receiver || !candidate.effect_roles.is_empty() { return Err(InferenceError::Boundary("native invocation needs source receiver or producer input roles")); }
                        let family = native.family; let signature = native.instance.ty; self.work_many(native.instance.effect_roots.len() + candidate.output_effect_roles.len())?;
                        let native = self.native_contract(contract)?; let roots = native.instance.effect_roots.clone(); let output_roles = self.candidate(native.candidate)?.output_effect_roles.clone();
                        let outputs = output_roles.into_iter().map(|(role, ordinal)| roots.get(ordinal as usize).copied().map(|summary| (role, summary)).ok_or(InferenceError::InvalidScheme)).collect::<Result<_, _>>()?;
                        (family, signature, outputs)
                    },
                    NativeAuthority::Family(contract) => { let family = self.native_family_contract(contract)?; (family.family, family.signature, Vec::new()) },
                };
                let TypeNode::Arrow(original) = self.clone_node(self.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme) };
                let mut actuals = vec![None; original.params.len()];
                for (argument, slot) in arguments.iter().zip(&binding.supplied_slots) { if *slot >= actuals.len() || actuals[*slot].is_some() { return Err(InferenceError::InvalidScheme); } actuals[*slot] = Some(argument.ty); }
                self.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: Some(authority), declared_error_bound: None, receiver: None, arguments: actuals, result: original.result, effects: original.effects, effect_bindings: Vec::new(), output_effect_bindings: outputs }, reason)?
            };
            let RequirementTemplate::Operation { family, call } = self.requirement(operation)?.template else { return Err(InferenceError::InvalidScheme) };
            self.solve_operation(operation, family, call)?;
            result.push(NativeInvocationAlternative { authority, operation });
        }
        Ok(result)
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    fn setup(graph: &mut InferenceContext, polymorphic: bool) -> (CandidateId, OperationFamilyId, ReasonId) {
        let span = Span::new(SourceId::new(0), 0, 1); let why = graph.reason(span, None).unwrap();
        let value = if polymorphic { graph.fresh(1, span).unwrap() } else { graph.atom(Atom::Str).unwrap() };
        let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![], result: value, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
        let candidate = graph.register_candidate(CandidateTemplate { identity: Name::intern("native-test"), public_label: Name::intern("native-test"), scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![], effect_roles: vec![], output_effect_roles: vec![], failure_projection: None }).unwrap();
        let family = graph.register_family(&[candidate]).unwrap(); (candidate, family, why)
    }
    #[test]
    fn native_value_retains_one_monomorphic_instance_and_retires_rolled_back_handles() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
        let mut graph = InferenceContext::default(); let (candidate, family, why) = setup(&mut graph, true);
        let instance = graph.instantiate(graph.candidate(candidate).unwrap().scheme, 1, why).unwrap();
        let signature = instance.ty; let native = graph.native_callable(candidate, family, instance).unwrap();
        assert_eq!(graph.callable_signature(native).unwrap(), signature);
        let TypeNode::NativeCallable(wrapper) = graph.node(native).unwrap() else { panic!() };
        let CallableAuthority::Native { authority: NativeAuthority::Single(contract) } = wrapper.alternatives[0] else { panic!() };
        assert_eq!(graph.native_contract_origin(contract).unwrap(), contract);
        let TypeNode::Arrow(arrow) = graph.node(signature).unwrap() else { panic!() }; let result = arrow.result;
        let int = graph.atom(Atom::Int).unwrap(); let string = graph.atom(Atom::Str).unwrap();
        graph.unify(result, int, why).unwrap(); assert!(graph.unify(result, string, why).is_err());
        let mut retired = None;
        graph.trial(|graph| { let instance = graph.instantiate(graph.candidate(candidate)?.scheme, 1, why)?; let value = graph.native_callable(candidate, family, instance)?; let TypeNode::NativeCallable(wrapper) = graph.node(value)? else { panic!() }; let CallableAuthority::Native { authority: NativeAuthority::Single(contract) } = wrapper.alternatives[0] else { panic!() }; retired = Some(contract); Ok(()) }).unwrap();
        assert!(graph.native_contract(retired.unwrap()).is_err());
    }
    #[test]
    fn native_invocation_uses_original_actuals_and_mono_signature_without_instantiation() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
        let mut graph = InferenceContext::default(); let span = Span::new(SourceId::new(0), 0, 1); let why = graph.reason(span, None).unwrap();
        let any = graph.atom(Atom::Any).unwrap(); let string = graph.atom(Atom::Str).unwrap(); let path = graph.atom(Atom::Path).unwrap();
        let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("value"), ty: any, defaulted: false, rest: false }], result: string, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
        let candidate = graph.register_candidate(CandidateTemplate { identity: Name::intern("native-json-test"), public_label: Name::intern("native-json-test"), scheme, has_receiver: false, actual_eligibility: vec![(0, Eligibility::JsonCompatible)], argument_relations: vec![], effect_roles: vec![], output_effect_roles: vec![], failure_projection: None }).unwrap();
        let family = graph.register_family(&[candidate]).unwrap(); let instance = graph.instantiate(scheme, 1, why).unwrap(); let native = graph.native_callable(candidate, family, instance).unwrap();
        let count = graph.counters.instantiations;
        let call = InvocationCall { callable: native, arguments: vec![InvocationArgument { kind: InvocationArgumentKind::Positional, ty: string }], result: string, domain: CallableDomain::Pure, effects: EffectSummary::Closed(EffectSet::EMPTY) };
        let requirement = graph.require_callable_invocation(call.clone(), why).unwrap(); graph.solve().unwrap();
        let evidence = graph.invocation_evidence(requirement).unwrap().unwrap(); assert_eq!(evidence.native_alternatives.len(), 1);
        let child = evidence.native_alternatives[0].operation;
        let RequirementTemplate::Operation { call, .. } = graph.requirement_template(child).unwrap() else { panic!() };
        assert_eq!(graph.operation_call(call).unwrap().arguments, vec![Some(string)]);
        assert_eq!(graph.counters.instantiations, count);
        graph.trial(|graph| { let mut call = InvocationCall { callable: native, arguments: vec![InvocationArgument { kind: InvocationArgumentKind::Named(Name::intern("value")), ty: path }], result: string, domain: CallableDomain::Pure, effects: EffectSummary::Closed(EffectSet::EMPTY) }; call.arguments[0].ty = path; graph.require_callable_invocation(call, why)?; graph.solve() }).unwrap_err();
        graph.freeze_scoped_with_facts(&[ScopedRoot { ty: native, scope: None }], &[], &[ScopedRequirementRoot { requirement, scope: None }]).unwrap();
    }
    #[test]
    fn native_user_join_preserves_alternatives_and_original_effect_summaries() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
        let mut graph = InferenceContext::default(); let (candidate, family, why) = setup(&mut graph, false);
        let instance = graph.instantiate(graph.candidate(candidate).unwrap().scheme, 1, why).unwrap(); let native = graph.native_callable(candidate, family, instance).unwrap();
        let string = graph.atom(Atom::Str).unwrap();
        let user = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![], result: string, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        for (left, right) in [(native, user), (user, native)] {
            let joined = graph.join_callable_values(left, right, 1, why).unwrap(); let TypeNode::NativeCallable(wrapper) = graph.node(joined).unwrap() else { panic!() };
            assert_eq!(wrapper.alternatives.len(), 2); assert!(wrapper.alternatives.contains(&CallableAuthority::User { signature: user, origin: user }));
        }
    }

    #[test]
    fn canonical_map_empty_native_value_is_monomorphic_and_outer_instances_are_fresh() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
        let mut graph = InferenceContext::default(); let span = Span::new(SourceId::new(0), 0, 1); let why = graph.reason(span, None).unwrap();
        let mut registry = crate::sema::registry_graph::RegistryGraph::default();
        let family = registry.module_family(&mut graph, "map", "empty", span).unwrap(); let candidate = graph.family(family).unwrap()[0]; let canonical = graph.candidate(candidate).unwrap().scheme;
        let instance = graph.instantiate(canonical, 2, why).unwrap(); let native = graph.native_callable(candidate, family, instance).unwrap();
        let signature = graph.callable_signature(native).unwrap(); let joined = graph.join_callable_values(native, signature, 2, why).unwrap();
        let row = graph.row(vec![RowField { label: Name::intern("first"), ty: native }, RowField { label: Name::intern("second"), ty: joined }], None).unwrap(); let root = graph.record(row).unwrap();
        let outer = graph.generalize(root, 0, Generalization::Allowed, &[]).unwrap();
        let mut instances = Vec::new(); let mut contracts = Vec::new();
        for _ in 0..2 {
            let instance = graph.instantiate(outer, 1, why).unwrap(); let TypeNode::Record(row) = graph.node(instance.ty).unwrap() else { panic!() }; let fields = graph.row_data(*row).unwrap().fields.clone();
            let mut handles = Vec::new(); for field in fields { let TypeNode::NativeCallable(wrapper) = graph.node(field.ty).unwrap() else { panic!() }; handles.extend(wrapper.alternatives.iter().filter_map(|authority| if let CallableAuthority::Native { authority: NativeAuthority::Single(contract) } = authority { Some(*contract) } else { None })); }
            assert_eq!(handles.len(), 2); assert_eq!(handles[0], handles[1]); contracts.push(handles[0]); instances.push(instance.ty);
        }
        assert_ne!(contracts[0], contracts[1]); assert_eq!(graph.native_contract_origin(contracts[0]).unwrap(), graph.native_contract_origin(contracts[1]).unwrap());
        let string = graph.atom(Atom::Str).unwrap(); let int = graph.atom(Atom::Int).unwrap(); let ints = graph.map(string, int).unwrap(); let strings = graph.map(string, string).unwrap();
        for (contract, result) in [(contracts[0], ints), (contracts[1], strings)] {
            let signature = graph.native_contract(contract).unwrap().instance.ty;
            let TypeNode::Arrow(arrow) = graph.node(signature).unwrap() else { panic!() }; let output = arrow.result;
            graph.unify(output, result, why).unwrap();
            let native = graph.allocate(TypeNode::NativeCallable(NativeCallable { signature, alternatives: vec![CallableAuthority::Native { authority: NativeAuthority::Single(contract) }] })).unwrap();
            let call = InvocationCall { callable: native, arguments: vec![], result, domain: CallableDomain::Pure, effects: EffectSummary::Closed(EffectSet::EMPTY) };
            graph.require_callable_invocation(call, why).unwrap(); graph.solve().unwrap();
            if result == ints { assert!(graph.unify(output, strings, why).is_err()); }
        }
        let roots = [ScopedRoot { ty: graph.scheme(outer).unwrap().body, scope: Some(outer) }, ScopedRoot { ty: instances[0], scope: None }, ScopedRoot { ty: instances[1], scope: None }];
        graph.freeze_scoped(&roots).unwrap();
    }
    #[test]
    fn native_external_effect_roots_freshen_without_losing_scope_or_bounds() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
        let mut graph = InferenceContext::default(); let (base, _, why) = setup(&mut graph, false); let signature = graph.scheme(graph.candidate(base).unwrap().scheme).unwrap().body;
        let port = EffectSummary::Variable(graph.fresh_effect_at(2, None).unwrap());
        let canonical = graph.generalize_with_effect_roots(signature, 0, Generalization::Allowed, &[], &[port]).unwrap();
        let candidate = graph.register_candidate(CandidateTemplate { identity: Name::intern("native-port-test"), public_label: Name::intern("native-port-test"), scheme: canonical, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![], effect_roles: vec![], output_effect_roles: vec![(ProducerRole::Pull, 0)], failure_projection: None }).unwrap(); let family = graph.register_family(&[candidate]).unwrap();
        let instance = graph.instantiate(canonical, 2, why).unwrap(); let native = graph.native_callable(candidate, family, instance).unwrap();
        let outer = graph.generalize(native, 0, Generalization::Allowed, &[]).unwrap(); let mut copies = Vec::new();
        for bits in [EffectSet::TIME, EffectSet::ENV] {
            let instance = graph.instantiate(outer, 1, why).unwrap(); let TypeNode::NativeCallable(wrapper) = graph.node(instance.ty).unwrap() else { panic!() }; let CallableAuthority::Native { authority: NativeAuthority::Single(contract) } = wrapper.alternatives[0] else { panic!() };
            let root = graph.native_contract(contract).unwrap().instance.effect_roots[0]; graph.equate_effects(root, EffectSummary::Closed(bits), why).unwrap(); copies.push((instance.ty, contract, bits));
        }
        assert_ne!(copies[0].1, copies[1].1);
        for (_, contract, bits) in &copies { assert_eq!(graph.resolved_effect_summary(graph.native_contract(*contract).unwrap().instance.effect_roots[0]).unwrap(), EffectSummary::Closed(*bits)); }
        let roots = [ScopedRoot { ty: graph.scheme(outer).unwrap().body, scope: Some(outer) }, ScopedRoot { ty: copies[0].0, scope: None }, ScopedRoot { ty: copies[1].0, scope: None }];
        graph.freeze_scoped(&roots).unwrap();
    }

    #[test]
    fn publication_rejects_native_child_output_ports_that_do_not_belong_to_its_monotype() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
        let mut graph = InferenceContext::default(); let span = Span::new(SourceId::new(0), 0, 1); let why = graph.reason(span, None).unwrap(); let mut registry = crate::sema::registry_graph::RegistryGraph::default();
        let family = registry.module_family(&mut graph, "process", "list", span).unwrap(); let candidate = graph.family(family).unwrap()[0]; let scheme = graph.candidate(candidate).unwrap().scheme;
        let instance = graph.instantiate(scheme, 1, why).unwrap(); let signature = instance.ty; let native = graph.native_callable(candidate, family, instance).unwrap();
        let TypeNode::Arrow(arrow) = graph.node(signature).unwrap() else { panic!() }; let result = arrow.result; let effects = arrow.effects;
        let requirement = graph.require_callable_invocation(InvocationCall { callable: native, arguments: vec![], result, effects, domain: CallableDomain::AnyCallable }, why).unwrap(); graph.solve().unwrap();
        let operation = graph.invocation_evidence(requirement).unwrap().unwrap().native_alternatives[0].operation;
        let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation).unwrap() else { panic!() };
        let (role, output) = graph.operation_calls[call.index()].value.output_effect_bindings.iter_mut().find(|(role, _)| *role == ProducerRole::Pull).unwrap(); assert_eq!(*role, ProducerRole::Pull); assert_eq!(*output, EffectSummary::Closed(EffectSet::PROCESS)); *output = EffectSummary::Closed(EffectSet::EMPTY);
        assert!(matches!(graph.freeze_scoped_with_facts(&[ScopedRoot { ty: native, scope: None }], &[], &[ScopedRequirementRoot { requirement, scope: None }]), Err(InferenceError::InvalidScheme)));
    }
    #[test]
    fn failed_native_allocation_rewinds_contracts_and_keeps_attempted_work_charged() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
        let mut graph = InferenceContext::default(); let (candidate, family, why) = setup(&mut graph, true); let instance = graph.instantiate(graph.candidate(candidate).unwrap().scheme, 1, why).unwrap();
        let contracts = graph.native_contracts.len(); let before = graph.counters.clone(); graph.limits.type_row_nodes = before.attempted_nodes as usize;
        assert!(matches!(graph.native_callable(candidate, family, instance), Err(InferenceError::Limit("type and row nodes"))));
        assert_eq!(graph.native_contracts.len(), contracts); assert_eq!(graph.counters.attempted_nodes, before.attempted_nodes + 1); assert!(graph.counters.work_units > before.work_units); assert!(graph.counters.attempted_constraints > before.attempted_constraints);
        assert!(graph.retained_storage().native_contract_bytes >= graph.native_contracts.capacity() * std::mem::size_of::<Slot<NativeContract>>());
    }

    #[test]
    fn native_command_family_keeps_one_choice_and_original_argument_domains() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
        let mut graph = InferenceContext::default(); let span = Span::new(SourceId::new(0), 0, 1); let why = graph.reason(span, None).unwrap(); let mut registry = crate::sema::registry_graph::RegistryGraph::default();
        let family = registry.module_family(&mut graph, "process", "command_argv", span).unwrap(); let candidates = graph.family(family).unwrap().to_vec(); assert_eq!(candidates.len(), 8);
        let mut instances = Vec::new(); for candidate in candidates { let scheme = graph.candidate(candidate).unwrap().scheme; instances.push((candidate, graph.instantiate(scheme, 1, why).unwrap())); }
        let native = graph.native_family_callable(family, instances, why).unwrap();
        let TypeNode::NativeCallable(wrapper) = graph.node(native).unwrap() else { panic!() }; let CallableAuthority::Native { authority: NativeAuthority::Family(contract) } = wrapper.alternatives[0] else { panic!() };
        let TypeNode::Arrow(signature) = graph.node(wrapper.signature).unwrap() else { panic!() }; assert!(signature.params[4].defaulted); let result = signature.result; let effects = signature.effects;
        assert!(matches!(graph.node(signature.params[0].ty).unwrap(), TypeNode::FiniteDomain(_)));
        let mut requirements = Vec::new(); let before = graph.counters.instantiations;
        for (target, item) in [(Atom::Str, Atom::Str), (Atom::Str, Atom::Path), (Atom::Path, Atom::Str), (Atom::Path, Atom::Path)] {
            let target = graph.atom(target).unwrap(); let item = graph.atom(item).unwrap(); let argv = graph.list(item).unwrap();
            let arguments = vec![InvocationArgument { kind: InvocationArgumentKind::Positional, ty: target }, InvocationArgument { kind: InvocationArgumentKind::Named(Name::intern("argv")), ty: argv }];
            let requirement = graph.require_callable_invocation(InvocationCall { callable: native, arguments, result, effects, domain: CallableDomain::Pure }, why).unwrap(); graph.solve().unwrap();
            let evidence = graph.invocation_evidence(requirement).unwrap().unwrap(); assert_eq!(evidence.native_alternatives.len(), 1); assert_eq!(evidence.native_alternatives[0].authority, NativeAuthority::Family(contract));
            let child = evidence.native_alternatives[0].operation; let candidate = graph.candidate_evidence(child).unwrap().unwrap().candidate; let member = graph.native_family_member(contract, candidate).unwrap();
            let selected = graph.native_contract(member).unwrap(); let TypeNode::Arrow(signature) = graph.node(selected.instance.ty).unwrap() else { panic!() }; assert_eq!(signature.params[0].ty, target);
            let TypeNode::List(actual) = graph.node(signature.params[1].ty).unwrap() else { panic!() }; assert_eq!(*actual, item); assert!(signature.params[4].defaulted);
            let RequirementTemplate::Operation { call, .. } = graph.requirement_template(child).unwrap() else { panic!() }; let actuals = &graph.operation_call(call).unwrap().arguments; assert_eq!(&actuals[..2], &[Some(target), Some(argv)]); assert!(actuals[2..].iter().all(Option::is_none)); assert_eq!(actuals.len(), 15); requirements.push(requirement);
        }
        assert_eq!(graph.counters.instantiations, before);
        let scoped = requirements.into_iter().map(|requirement| ScopedRequirementRoot { requirement, scope: None }).collect::<Vec<_>>(); let solved = graph.freeze_scoped_with_facts(&[ScopedRoot { ty: native, scope: None }], &[], &scoped).unwrap(); solved.validate_native_family_contract_scoped(contract, None).unwrap();
    }

    fn command_family(graph: &mut InferenceContext, why: ReasonId, span: Span) -> TypeId {
        let mut registry = crate::sema::registry_graph::RegistryGraph::default();
        let family = registry.module_family(graph, "process", "command_argv", span).unwrap();
        let candidates = graph.family(family).unwrap().to_vec();
        let mut instances = Vec::new(); for candidate in candidates { let scheme = graph.candidate(candidate).unwrap().scheme; instances.push((candidate, graph.instantiate(scheme, 1, why).unwrap())); }
        graph.native_family_callable(family, instances, why).unwrap()
    }
    fn family_authority(graph: &InferenceContext, callable: TypeId) -> NativeFamilyContractId {
        let TypeNode::NativeCallable(callable) = graph.node(callable).unwrap() else { panic!() };
        let CallableAuthority::Native { authority: NativeAuthority::Family(id) } = callable.alternatives[0] else { panic!() }; id
    }
    #[test]
    fn family_outer_copies_share_one_replacement_and_retire_trial_handles() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
        let mut graph = InferenceContext::default(); let span = Span::new(SourceId::new(0), 0, 1); let why = graph.reason(span, None).unwrap();
        let native = command_family(&mut graph, why, span); let original = family_authority(&graph, native);
        let signature = graph.callable_signature(native).unwrap(); let joined = graph.join_callable_values(native, signature, 1, why).unwrap();
        let row = graph.row(vec![RowField { label: Name::intern("left"), ty: native }, RowField { label: Name::intern("right"), ty: joined }], None).unwrap(); let root = graph.record(row).unwrap();
        let scheme = graph.generalize(root, 0, Generalization::Allowed, &[]).unwrap(); let mut previous = None;
        for _ in 0..2 {
            let instance = graph.instantiate(scheme, 1, why).unwrap(); let TypeNode::Record(row) = graph.node(instance.ty).unwrap() else { panic!() }; let fields = graph.row_data(*row).unwrap().fields.clone();
            let ids = fields.iter().map(|field| graph.node(field.ty).unwrap()).map(|node| { let TypeNode::NativeCallable(callable) = node else { panic!() }; callable.alternatives.iter().find_map(|authority| match authority { CallableAuthority::Native { authority: NativeAuthority::Family(id) } => Some(*id), _ => None }).unwrap() }).collect::<Vec<_>>();
            assert_eq!(ids[0], ids[1]); assert_ne!(ids[0], original); assert_ne!(Some(ids[0]), previous); previous = Some(ids[0]);
            assert_eq!(graph.native_family_origin(ids[0]).unwrap(), original);
            let members = graph.native_family_contract(ids[0]).unwrap().members.clone(); let originals = graph.native_family_contract(original).unwrap().members.clone();
            for (member, original) in members.iter().zip(&originals) { assert_ne!(member, original); assert_eq!(graph.native_contract_origin(*member).unwrap(), *original); }
        }
        let before = graph.counters().work_units; let count = graph.native_families.len(); let mut retired = None;
        graph.trial(|graph| { let instance = graph.instantiate(scheme, 1, why)?; let TypeNode::Record(row) = graph.node(instance.ty)? else { panic!() }; retired = Some(family_authority(graph, graph.row_data(*row)?.fields[0].ty)); Ok(()) }).unwrap();
        assert_eq!(graph.native_families.len(), count); assert!(graph.native_family_contract(retired.unwrap()).is_err()); assert!(graph.counters().work_units > before);
        let storage = graph.retained_storage(); assert!(storage.native_family_bytes >= graph.native_families.capacity() * std::mem::size_of::<Slot<NativeFamilyContract>>());
    }
    #[test]
    fn family_pending_inputs_are_not_trained_and_bytes_stdin_uses_required_member() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
        let mut graph = InferenceContext::default(); let span = Span::new(SourceId::new(0), 0, 1); let why = graph.reason(span, None).unwrap(); let native = command_family(&mut graph, why, span);
        let TypeNode::Arrow(signature) = graph.clone_node(graph.callable_signature(native).unwrap()).unwrap() else { panic!() }; let target = graph.fresh(1, span).unwrap(); let argv = graph.fresh(1, span).unwrap();
        let req = graph.require_callable_invocation(InvocationCall { callable: native, arguments: vec![InvocationArgument { kind: InvocationArgumentKind::Positional, ty: target }, InvocationArgument { kind: InvocationArgumentKind::Positional, ty: argv }], result: signature.result, effects: signature.effects, domain: CallableDomain::Pure }, why).unwrap(); graph.solve().unwrap();
        assert!(graph.invocation_evidence(req).unwrap().is_none()); let child = graph.native_invocation_children(req).unwrap()[0].operation; assert!(graph.candidate_evidence(child).unwrap().is_none()); assert_eq!(graph.resolved(target).unwrap(), target); assert_eq!(graph.resolved(argv).unwrap(), argv);
        let path = graph.atom(Atom::Path).unwrap(); let list = graph.list(path).unwrap(); graph.unify(target,path,why).unwrap(); graph.unify(argv,list,why).unwrap(); graph.solve().unwrap(); assert!(graph.candidate_evidence(child).unwrap().is_some());
        let bytes = graph.atom(Atom::Bytes).unwrap(); let req = graph.require_callable_invocation(InvocationCall { callable: native, arguments: vec![InvocationArgument { kind: InvocationArgumentKind::Named(Name::intern("stdin")), ty: bytes }, InvocationArgument { kind: InvocationArgumentKind::Named(Name::intern("argv")), ty: list }, InvocationArgument { kind: InvocationArgumentKind::Named(Name::intern("target")), ty: path }], result: signature.result, effects: signature.effects, domain: CallableDomain::Pure }, why).unwrap(); graph.solve().unwrap();
        let child = graph.invocation_evidence(req).unwrap().unwrap().native_alternatives[0].operation; let selected = graph.candidate_evidence(child).unwrap().unwrap(); let TypeNode::Arrow(member) = graph.node(selected.signature).unwrap() else { panic!() }; assert_eq!(member.params[4].ty,bytes); assert!(!member.params[4].defaulted);
    }
    #[test]
    fn family_publication_rejects_forged_envelope_and_missing_correlated_member() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter(); let span = Span::new(SourceId::new(0), 0, 1);
        for corrupt_default in [false, true] {
            let mut graph = InferenceContext::default(); let why = graph.reason(span, None).unwrap(); let native = command_family(&mut graph, why, span); let id = family_authority(&graph,native);
            if corrupt_default { let signature = graph.callable_signature(native).unwrap(); let TypeNode::Arrow(arrow) = &mut graph.nodes[signature.index()].value else { panic!() }; arrow.params[4].defaulted = false; }
            else { graph.native_families[id.index()].value.members.pop(); }
            assert!(matches!(graph.freeze_scoped(&[ScopedRoot { ty:native, scope:None }]),Err(InferenceError::InvalidScheme)));
        }
    }
    #[test]
    fn native_declared_erasure_keeps_original_container_leaf_until_guard_discharge() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter(); let mut graph = InferenceContext::default(); let span = Span::new(SourceId::new(0),0,1); let why = graph.reason(span,None).unwrap();
        let mut registry = crate::sema::registry_graph::RegistryGraph::default(); let family = registry.module_family(&mut graph,"json","encode_lines",span).unwrap(); let candidate = graph.family(family).unwrap()[0]; let scheme = graph.candidate(candidate).unwrap().scheme; let instance = graph.instantiate(scheme,1,why).unwrap(); let native = graph.native_callable(candidate,family,instance).unwrap();
        let value = graph.fresh(1,span).unwrap(); let result = graph.fresh(1,span).unwrap(); let req = graph.require_callable_invocation(InvocationCall {callable:native,arguments:vec![InvocationArgument{kind:InvocationArgumentKind::Named(Name::intern("values")),ty:value}],result,effects:EffectSummary::Closed(EffectSet::EMPTY),domain:CallableDomain::Pure},why).unwrap(); graph.solve().unwrap(); assert_eq!(graph.resolved(value).unwrap(),value);
        let child = graph.invocation_evidence(req).unwrap().unwrap().native_alternatives[0].operation; assert!(graph.candidate_evidence(child).unwrap().is_none());
        let arrow = graph.arrow(Arrow {kind:CallableKind::Pure,params:vec![Parameter{label:Name::intern("value"),ty:value,defaulted:false,rest:false}],result,effects:EffectSummary::Closed(EffectSet::EMPTY)}).unwrap(); let outer = graph.generalize(arrow,0,Generalization::Allowed,&[req]).unwrap();
        let bool_ty = graph.atom(Atom::Bool).unwrap(); let row = graph.row(vec![RowField{label:Name::intern("valid"),ty:bool_ty}],None).unwrap(); let record = graph.record(row).unwrap(); let valid = graph.list(record).unwrap(); let path = graph.atom(Atom::Path).unwrap(); let invalid = graph.list(path).unwrap();
        for (actual, accepted) in [(valid,true),(invalid,false)] { let instance = graph.instantiate(outer,1,why).unwrap(); let TypeNode::Arrow(arrow) = graph.clone_node(instance.ty).unwrap() else {panic!()}; assert_eq!(graph.trial(|graph|{graph.unify(arrow.params[0].ty,actual,why)?;graph.solve()}).is_ok(),accepted); }
    }

    fn retained_family(graph: &mut InferenceContext, module: &str, name: &str, why: ReasonId, span: Span) -> TypeId {
        let mut registry = crate::sema::registry_graph::RegistryGraph::default(); let family = registry.module_family(graph,module,name,span).unwrap();
        let candidates = graph.family(family).unwrap().to_vec(); let mut instances = Vec::new();
        for candidate in candidates { let scheme = graph.candidate(candidate).unwrap().scheme; instances.push((candidate,graph.instantiate(scheme,1,why).unwrap())); }
        graph.native_family_callable(family,instances,why).unwrap()
    }
    #[test]
    fn repeated_conditional_family_authority_keeps_its_member_tuples_once() {
        let symbols=crate::symbol::SymbolOwner::new(); let _guard=symbols.enter(); let span=Span::new(SourceId::new(0),0,1);
        let mut graph=InferenceContext::default(); let why=graph.reason(span,None).unwrap();
        let native=retained_family(&mut graph,"process","ports",why,span);
        let joined=graph.join_callable_values(native,native,1,why).unwrap();
        let TypeNode::NativeCallable(wrapper)=graph.node(joined).unwrap() else {panic!()};
        assert_eq!(wrapper.alternatives.len(),1);
        graph.freeze_scoped(&[ScopedRoot {ty:joined,scope:None}]).unwrap();
    }
    #[test]
    fn producer_callable_choices_preserve_zero_and_one_argument_member_contracts() {
        let symbols=crate::symbol::SymbolOwner::new(); let _guard=symbols.enter(); let span=Span::new(SourceId::new(0),0,1);
        for name in ["ports","threads"] {
            let mut graph=InferenceContext::default(); let why=graph.reason(span,None).unwrap(); let native=retained_family(&mut graph,"process",name,why,span); let int=graph.atom(Atom::Int).unwrap(); let count=graph.counters.instantiations;
            let mut requirements=Vec::new();
            for arguments in [Vec::new(),vec![InvocationArgument{kind:InvocationArgumentKind::Named(Name::intern("pid")),ty:int}]] {
                let result=graph.fresh(0,span).unwrap(); let effects=EffectSummary::Variable(graph.fresh_derived_effect_at(0,None).unwrap()); let supplied=arguments.len();
                let req=graph.require_callable_invocation(InvocationCall{callable:native,arguments,result,effects,domain:CallableDomain::AnyCallable},why).unwrap(); graph.solve().unwrap();
                let evidence=graph.invocation_evidence(req).unwrap().unwrap(); let TypeNode::Arrow(signature)=graph.node(evidence.unique_plan().unwrap().0).unwrap() else{panic!()}; assert_eq!(signature.params.len(),supplied);
                let TypeNode::Result(success,_)=graph.node(graph.resolved(result).unwrap()).unwrap() else{panic!()}; assert!(matches!(graph.node(*success).unwrap(),TypeNode::Stream(_)));
                let child=evidence.native_alternatives[0].operation; let selected=graph.candidate_evidence(child).unwrap().unwrap(); assert_eq!(selected.effect_roots.len(),2); requirements.push(ScopedRequirementRoot{requirement:req,scope:None});
            }
            assert_eq!(graph.counters.instantiations,count); graph.freeze_scoped_with_facts(&[ScopedRoot{ty:native,scope:None}],&[],&requirements).unwrap();
        }
    }
    #[test]
    fn hash_callable_choices_keep_actual_labels_results_and_permission_domains() {
        let symbols=crate::symbol::SymbolOwner::new(); let _guard=symbols.enter(); let span=Span::new(SourceId::new(0),0,1); let mut graph=InferenceContext::default(); let why=graph.reason(span,None).unwrap(); let native=retained_family(&mut graph,"hash","sha256",why,span);
        let mut requirements=Vec::new(); let count=graph.counters.instantiations;
        for (atom,label,domain) in [(Atom::Bytes,"data",CallableDomain::Pure),(Atom::Path,"path",CallableDomain::AnyCallable)] {
            let actual=graph.atom(atom).unwrap(); let result=graph.fresh(0,span).unwrap(); let effects=EffectSummary::Variable(graph.fresh_derived_effect_at(0,None).unwrap());
            let req=graph.require_callable_invocation(InvocationCall{callable:native,arguments:vec![InvocationArgument{kind:InvocationArgumentKind::Named(Name::intern(label)),ty:actual}],result,effects,domain},why).unwrap(); graph.solve().unwrap();
            let evidence=graph.invocation_evidence(req).unwrap().unwrap(); let TypeNode::Arrow(signature)=graph.node(evidence.unique_plan().unwrap().0).unwrap() else{panic!()}; assert_eq!(signature.params[0].label,Name::intern(label)); assert_eq!(graph.resolved_effect_summary(effects).unwrap(),graph.resolved_effect_summary(signature.effects).unwrap());
            assert_eq!(matches!(graph.node(graph.resolved(result).unwrap()).unwrap(),TypeNode::Result(_, _)),atom==Atom::Path); requirements.push(ScopedRequirementRoot{requirement:req,scope:None});
        }
        let path=graph.atom(Atom::Path).unwrap(); graph.trial(|graph|{let result=graph.fresh(0,span)?;let req=graph.require_callable_invocation(InvocationCall{callable:native,arguments:vec![InvocationArgument{kind:InvocationArgumentKind::Positional,ty:path}],result,effects:EffectSummary::Closed(EffectSet::EMPTY),domain:CallableDomain::Pure},why)?;graph.solve()?;graph.invocation_evidence(req)?;Ok(())}).unwrap_err();
        assert_eq!(graph.counters.instantiations,count); graph.freeze_scoped_with_facts(&[ScopedRoot{ty:native,scope:None}],&[],&requirements).unwrap();
    }

    #[test]
    fn json_callable_choice_uses_original_arity_instead_of_inventing_a_fallback_default() {
        let symbols=crate::symbol::SymbolOwner::new(); let _guard=symbols.enter(); let span=Span::new(SourceId::new(0),0,1); let mut graph=InferenceContext::default(); let why=graph.reason(span,None).unwrap(); let native=retained_family(&mut graph,"json","get",why,span);
        let any=graph.atom(Atom::Any).unwrap(); let string=graph.atom(Atom::Str).unwrap(); let path=graph.list(string).unwrap(); let count=graph.counters.instantiations; let mut requirements=Vec::new();
        for fallback in [false,true] {
            let mut arguments=vec![InvocationArgument{kind:InvocationArgumentKind::Named(Name::intern("value")),ty:any},InvocationArgument{kind:InvocationArgumentKind::Named(Name::intern("path")),ty:path}];if fallback {arguments.push(InvocationArgument{kind:InvocationArgumentKind::Named(Name::intern("fallback")),ty:string});}
            let result=graph.fresh(0,span).unwrap();let effects=EffectSummary::Variable(graph.fresh_derived_effect_at(0,None).unwrap());let req=graph.require_callable_invocation(InvocationCall{callable:native,arguments,result,effects,domain:CallableDomain::Pure},why).unwrap();graph.solve().unwrap();
            let evidence=graph.invocation_evidence(req).unwrap().unwrap();let TypeNode::Arrow(arrow)=graph.node(evidence.unique_plan().unwrap().0).unwrap()else{panic!()};assert_eq!(arrow.params.len(),if fallback{3}else{2});assert!(evidence.unique_plan().unwrap().1.default_slots.is_empty());
            let actual=graph.node(graph.resolved(result).unwrap()).unwrap();assert_eq!(matches!(actual,TypeNode::Result(_,_)),!fallback);if fallback {assert_eq!(actual,&TypeNode::Atom(Atom::Any));}
            requirements.push(ScopedRequirementRoot{requirement:req,scope:None});
        }
        assert_eq!(graph.counters.instantiations,count);graph.freeze_scoped_with_facts(&[ScopedRoot{ty:native,scope:None}],&[],&requirements).unwrap();
    }
    #[test]
    fn pending_effect_choice_keeps_computed_effect_symbolic_and_fresh_per_outer_call() {
        let symbols=crate::symbol::SymbolOwner::new();let _guard=symbols.enter();let span=Span::new(SourceId::new(0),0,1);let mut graph=InferenceContext::default();let why=graph.reason(span,None).unwrap();let mut candidates=Vec::new();
        for (atom,bits,label) in [(Atom::Int,EffectSet::TIME,"time-choice"),(Atom::Str,EffectSet::ENV,"env-choice")] {
            let ty=graph.atom(atom).unwrap();let arrow=graph.arrow(Arrow{kind:CallableKind::Proc,params:vec![Parameter{label:Name::intern("argument"),ty,defaulted:false,rest:false}],result:ty,effects:EffectSummary::Closed(bits)}).unwrap();let scheme=graph.generalize(arrow,0,Generalization::Allowed,&[]).unwrap();
            let candidate=graph.register_candidate(CandidateTemplate{identity:Name::intern(label),public_label:Name::intern(label),scheme,has_receiver:false,actual_eligibility:Vec::new(),argument_relations:Vec::new(),effect_roles:Vec::new(),output_effect_roles:Vec::new(),failure_projection:None}).unwrap();candidates.push(candidate);
        }
        let family=graph.register_family(&candidates).unwrap();let mut instances=Vec::new();for candidate in candidates {let scheme=graph.candidate(candidate).unwrap().scheme;instances.push((candidate,graph.instantiate(scheme,1,why).unwrap()));}let native=graph.native_family_callable(family,instances,why).unwrap();
        let argument=graph.fresh(1,span).unwrap();let result=graph.fresh(1,span).unwrap();let effects=EffectSummary::Variable(graph.fresh_derived_effect_at(1,None).unwrap());let req=graph.require_callable_invocation(InvocationCall{callable:native,arguments:vec![InvocationArgument{kind:InvocationArgumentKind::Positional,ty:argument}],result,effects,domain:CallableDomain::AnyCallable},why).unwrap();graph.solve().unwrap();
        assert!(graph.invocation_evidence(req).unwrap().is_none());assert_eq!(graph.resolved(argument).unwrap(),argument);graph.seal_derived_effects(&[effects]).unwrap();assert!(matches!(graph.resolved_effect_summary(effects).unwrap(),EffectSummary::Variable(_)));assert_eq!(graph.native_invocation_children(req).unwrap().len(),1);
        let original_child = graph.native_invocation_children(req).unwrap()[0].operation;
        let arrow=graph.arrow(Arrow{kind:CallableKind::Proc,params:vec![Parameter{label:Name::intern("argument"),ty:argument,defaulted:false,rest:false}],result,effects}).unwrap();let scheme=graph.generalize(arrow,0,Generalization::Allowed,&[req]).unwrap();
        let mut summaries=Vec::new();for atom in [Atom::Int,Atom::Str] {let instance=graph.instantiate(scheme,0,why).unwrap();let TypeNode::Arrow(arrow)=graph.clone_node(instance.ty).unwrap()else{panic!()};let actual=graph.atom(atom).unwrap();graph.unify(arrow.params[0].ty,actual,why).unwrap();graph.solve().unwrap();let invocation=graph.invocation_evidence(instance.requirements[0]).unwrap().unwrap();assert!(matches!(graph.node(invocation.unique_plan().unwrap().0).unwrap(),TypeNode::Arrow(_)));let child = invocation.native_alternatives[0].operation; let pairs = graph.requirement_correspondences(&instance.requirements).unwrap(); assert!(pairs.contains(&(original_child,child)), "known native body child must retain immediate ancestry"); let RequirementTemplate::CallableInvocation { call: parent } = graph.requirement_template(instance.requirements[0]).unwrap() else { panic!() }; let RequirementTemplate::Operation { call: child_call, .. } = graph.requirement_template(child).unwrap() else { panic!() }; assert_eq!(graph.operation_call(child_call).unwrap().binding, OperationBinding::Invocation(parent)); summaries.push(graph.resolved_effect_summary(arrow.effects).unwrap());}
        assert_ne!(summaries[0],summaries[1]);assert_eq!(summaries[0],EffectSummary::Closed(EffectSet::TIME));
    }

    #[test]
    fn callback_protocol_invokes_original_callable_choice_with_real_binding() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter(); let span = Span::new(SourceId::new(0), 0, 1);
        let mut graph = InferenceContext::default(); let why = graph.reason(span, None).unwrap(); let native = retained_family(&mut graph, "process", "ports", why, span);
        let int = graph.atom(Atom::Int).unwrap(); let result = graph.fresh(1, span).unwrap(); let effects = EffectSummary::Variable(graph.fresh_effect_at(1, None).unwrap());
        let protocol = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("<item>"), ty: int, defaulted: false, rest: false }], result, effects }).unwrap();
        let operation = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("callback"), ty: protocol, defaulted: false, rest: false }], result, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let scheme = graph.generalize(operation, 0, Generalization::Allowed, &[]).unwrap();
        let candidate = graph.register_candidate(CandidateTemplate { identity: Name::intern("callback-protocol"), public_label: Name::intern("invoke-item"), scheme, has_receiver: false, actual_eligibility: Vec::new(), argument_relations: vec![ArgumentRelation::InvocationProtocol], effect_roles: Vec::new(), output_effect_roles: Vec::new(), failure_projection: None }).unwrap();
        let family = graph.register_family(&[candidate]).unwrap(); let result = graph.fresh(0, span).unwrap();
        let requirement = graph.require_operation(family, OperationCall { binding: OperationBinding::Slots, effect_mode: OperationEffectMode::AvailableBudget, mono_authority: None, receiver: None, arguments: vec![Some(native)], declared_error_bound: None, result, effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: Vec::new(), output_effect_bindings: Vec::new() }, why).unwrap();
        let contracts = graph.native_contracts.len(); graph.solve().unwrap();
        let evidence = graph.candidate_evidence(requirement).unwrap().unwrap(); assert_eq!(evidence.callback_invocations.len(), 1); let receipt = evidence.callback_invocations[0]; assert_eq!(receipt.slot, 0);
        let RequirementTemplate::CallableInvocation { call } = graph.requirement_template(receipt.invocation).unwrap() else { panic!() };
        let original = graph.invocation_call(call).unwrap(); assert_eq!(original.callable, native); assert_eq!(original.domain, CallableDomain::Exact(CallableKind::Proc)); assert_eq!(original.arguments, vec![InvocationArgument { kind: InvocationArgumentKind::Positional, ty: int }]);
        let invoked = graph.invocation_evidence(receipt.invocation).unwrap().unwrap(); let TypeNode::Arrow(arrow) = graph.node(invoked.unique_plan().unwrap().0).unwrap() else { panic!() }; assert_eq!(arrow.params.len(), 1); assert_eq!(arrow.params[0].label, Name::intern("pid")); assert_eq!(invoked.unique_plan().unwrap().1.supplied_slots, vec![0]);
        assert_eq!(graph.native_contracts.len(), contracts);
        graph.freeze_scoped_with_facts(&[ScopedRoot { ty: native, scope: None }, ScopedRoot { ty: result, scope: None }], &[], &[ScopedRequirementRoot { requirement, scope: None }]).unwrap();
    }

    #[test]
    fn callback_protocol_keeps_actual_defaults_rest_and_exact_kind_contracts() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter(); let span = Span::new(SourceId::new(0), 0, 1);
        let mut graph = InferenceContext::default(); let why = graph.reason(span, None).unwrap(); let int = graph.atom(Atom::Int).unwrap(); let empty = EffectSummary::Closed(EffectSet::EMPTY);
        let protocol = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("<item>"), ty: int, defaulted: false, rest: false }], result: int, effects: empty }).unwrap();
        let arrow = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("callback"), ty: protocol, defaulted: false, rest: false }], result: int, effects: empty }).unwrap(); let scheme = graph.generalize(arrow, 0, Generalization::Allowed, &[]).unwrap();
        let candidate = graph.register_candidate(CandidateTemplate { identity: Name::intern("callback-binding-contract"), public_label: Name::intern("invoke-item"), scheme, has_receiver: false, actual_eligibility: Vec::new(), argument_relations: vec![ArgumentRelation::InvocationProtocol], effect_roles: Vec::new(), output_effect_roles: Vec::new(), failure_projection: None }).unwrap(); let family = graph.register_family(&[candidate]).unwrap();
        let list = graph.list(int).unwrap(); let string = graph.atom(Atom::Str).unwrap();
        for (kind, parameters, accepted, defaults, rest) in [
            (CallableKind::Proc, vec![Parameter { label: Name::intern("input"), ty: int, defaulted: false, rest: false }, Parameter { label: Name::intern("suffix"), ty: int, defaulted: true, rest: false }], true, vec![1], None),
            (CallableKind::Proc, vec![Parameter { label: Name::intern("items"), ty: list, defaulted: false, rest: true }], true, vec![], Some(0)),
            (CallableKind::Proc, vec![Parameter { label: Name::intern("input"), ty: int, defaulted: false, rest: false }, Parameter { label: Name::intern("required"), ty: int, defaulted: false, rest: false }], false, vec![], None),
            (CallableKind::Proc, vec![Parameter { label: Name::intern("input"), ty: string, defaulted: false, rest: false }], false, vec![], None),
            (CallableKind::Pure, vec![Parameter { label: Name::intern("input"), ty: int, defaulted: false, rest: false }], false, vec![], None),
        ] {
            let actual = graph.arrow(Arrow { kind, params: parameters, result: int, effects: empty }).unwrap();
            let outcome = graph.trial(|graph| {
                let requirement = graph.require_operation(family, OperationCall { binding: OperationBinding::Slots, effect_mode: OperationEffectMode::AvailableBudget, mono_authority: None, receiver: None, arguments: vec![Some(actual)], declared_error_bound: None, result: int, effects: empty, effect_bindings: Vec::new(), output_effect_bindings: Vec::new() }, why)?; graph.solve()?;
                let callback = graph.candidate_callback_invocation(requirement, 0)?.unwrap(); let evidence = graph.invocation_evidence(callback)?.unwrap(); assert_eq!(evidence.unique_plan().unwrap().1.default_slots, defaults); assert_eq!(evidence.unique_plan().unwrap().1.rest_slot, rest); assert_eq!(evidence.unique_plan().unwrap().2, InvocationDefaultTiming::AtCall); Ok(())
            });
            assert_eq!(outcome.is_ok(), accepted);
        }
    }

    #[test]
    fn callback_protocol_publication_rejects_altered_formal_slot_receipt() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter(); let span = Span::new(SourceId::new(0), 0, 1);
        let mut graph = InferenceContext::default(); let why = graph.reason(span, None).unwrap(); let int = graph.atom(Atom::Int).unwrap(); let empty = EffectSummary::Closed(EffectSet::EMPTY);
        let callback = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("actual"), ty: int, defaulted: false, rest: false }], result: int, effects: empty }).unwrap();
        let protocol = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("<item>"), ty: int, defaulted: false, rest: false }], result: int, effects: empty }).unwrap();
        let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("callback"), ty: protocol, defaulted: false, rest: false }], result: int, effects: empty }).unwrap(); let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
        let candidate = graph.register_candidate(CandidateTemplate { identity: Name::intern("callback-published-receipt"), public_label: Name::intern("invoke-item"), scheme, has_receiver: false, actual_eligibility: Vec::new(), argument_relations: vec![ArgumentRelation::InvocationProtocol], effect_roles: Vec::new(), output_effect_roles: Vec::new(), failure_projection: None }).unwrap(); let family = graph.register_family(&[candidate]).unwrap();
        let requirement = graph.require_operation(family, OperationCall { binding: OperationBinding::Slots, effect_mode: OperationEffectMode::AvailableBudget, mono_authority: None, receiver: None, arguments: vec![Some(callback)], declared_error_bound: None, result: int, effects: empty, effect_bindings: Vec::new(), output_effect_bindings: Vec::new() }, why).unwrap(); graph.solve().unwrap();
        graph.requirements[requirement.index()].value.candidate.as_mut().unwrap().callback_invocations[0].slot = 1;
        assert!(matches!(graph.freeze_scoped_with_facts(&[ScopedRoot { ty: callback, scope: None }], &[], &[ScopedRequirementRoot { requirement, scope: None }]), Err(InferenceError::InvalidScheme)));
    }

    #[test]
    fn captured_uniform_native_family_replays_original_children_through_outer_scheme() {
        let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter(); let span = Span::new(SourceId::new(0), 0, 1);
        let mut graph = InferenceContext::default(); let why = graph.reason(span, None).unwrap(); let native = command_family(&mut graph, why, span);
        let target = graph.fresh(1, span).unwrap(); let argv = graph.fresh(1, span).unwrap(); let command = graph.atom(Atom::Command).unwrap(); let empty = EffectSummary::Closed(EffectSet::EMPTY);
        let invocation = graph.require_callable_invocation(InvocationCall { callable: native, arguments: vec![InvocationArgument { kind: InvocationArgumentKind::Positional, ty: target }, InvocationArgument { kind: InvocationArgumentKind::Positional, ty: argv }], result: command, effects: empty, domain: CallableDomain::Pure }, why).unwrap(); graph.solve().unwrap();
        let source_child = graph.native_invocation_children(invocation).unwrap()[0].operation;
        let arrow = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("target"), ty: target, defaulted: false, rest: false }, Parameter { label: Name::intern("arguments"), ty: argv, defaulted: false, rest: false }], result: command, effects: empty }).unwrap(); let scheme = graph.generalize(arrow, 0, Generalization::Allowed, &[invocation]).unwrap();
        let string = graph.atom(Atom::Str).unwrap(); let path = graph.atom(Atom::Path).unwrap(); let strings = graph.list(string).unwrap(); let paths = graph.list(path).unwrap();
        let mut roots = vec![ScopedRoot { ty: native, scope: None }, ScopedRoot { ty: graph.scheme(scheme).unwrap().body, scope: Some(scheme) }];
        let mut requirements = vec![ScopedRequirementRoot { requirement: invocation, scope: Some(scheme) }];
        for (target, argv) in [(string, paths), (path, strings)] {
            let instance = graph.instantiate(scheme, 0, why).unwrap(); let TypeNode::Arrow(arrow) = graph.clone_node(instance.ty).unwrap() else { panic!() };
            graph.unify(arrow.params[0].ty, target, why).unwrap(); graph.unify(arrow.params[1].ty, argv, why).unwrap(); graph.solve().unwrap();
            let child = graph.native_invocation_children(instance.requirements[0]).unwrap()[0].operation;
            assert!(graph.requirement_correspondences(&instance.requirements).unwrap().contains(&(source_child, child)));
            assert!(graph.candidate_evidence(child).unwrap().is_some());
            roots.push(ScopedRoot { ty: instance.ty, scope: None }); requirements.push(ScopedRequirementRoot { requirement: instance.requirements[0], scope: None });
        }
        graph.freeze_scoped_with_facts(&roots, &[], &requirements).unwrap();
    }

}

#[cfg(test)]
mod conditional_tests {
    use super::*;
    use crate::source::SourceId;

    #[test]
    fn conditional_invocation_keeps_each_dynamic_range_default_and_origin() {
        fn publish(alter:u8) -> Result<SolvedGraph,InferenceError> {
            let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter();
            let mut graph=InferenceContext::default(); let string=graph.atom(Atom::Str)?; let list=graph.list(string)?;
            let parameter=|name,defaulted|Parameter {label:Name::intern(name),ty:string,defaulted,rest:false};
            let first=graph.arrow(Arrow {kind:CallableKind::Proc,params:vec![parameter("input",false)],result:string,effects:EffectSummary::Closed(EffectSet::TIME)})?;
            let second=graph.arrow(Arrow {kind:CallableKind::Proc,params:vec![parameter("source",false),parameter("fallback",true)],result:string,effects:EffectSummary::Closed(EffectSet::ENV)})?;
            let foreign=graph.arrow(Arrow {kind:CallableKind::Proc,params:vec![parameter("input",false)],result:string,effects:EffectSummary::Closed(EffectSet::TIME)})?;
            let why=graph.reason(Span::at(SourceId::new(0),0),None)?;
            let callable=graph.join_callable_values(first,second,0,why)?;
            let effects=EffectSummary::Closed(EffectSet(EffectSet::TIME.0|EffectSet::ENV.0));
            let requirement=graph.require_callable_invocation(InvocationCall {callable,arguments:vec![InvocationArgument {kind:InvocationArgumentKind::PositionalSplice,ty:list}],result:string,effects,domain:CallableDomain::AnyCallable},why)?;
            graph.solve()?;
            let evidence=graph.requirements[requirement.index()].value.invocation.as_mut().unwrap();
            assert!(evidence.unique_plan().is_none());
            let InvocationPlan::All {branches}=&mut evidence.plan else {unreachable!()};
            assert_eq!(branches.len(),2); assert!(branches.iter().all(|branch|branch.binding.supplied_slots.is_empty()));
            assert_eq!(branches.iter().map(|branch|branch.binding.dynamic.as_ref().unwrap().segments.len()).collect::<Vec<_>>(),vec![1,1]);
            match alter {
                1=>{branches.pop();},
                2=>{branches[0].binding.dynamic.as_mut().unwrap().required_slots.clear();},
                3=>{branches[0].effects=EffectSummary::Closed(EffectSet::EMPTY);},
                4=>{let CallableAuthority::User {origin,..}=&mut branches[0].authority else {unreachable!()};*origin=foreign;},
                _=>{},
            }
            graph.freeze_scoped_with_facts(&[ScopedRoot {ty:callable,scope:None}],&[],&[ScopedRequirementRoot {requirement,scope:None}])
        }
        assert!(publish(0).is_ok());
        for alter in 1..=4 {assert!(matches!(publish(alter),Err(InferenceError::InvalidScheme)),"alteration {alter}");}
    }
}

#[cfg(test)]
mod conditional_effect_tests {
    use super::*;
    use crate::source::SourceId;
    #[test]
    fn conditional_callback_protocol_reports_the_original_operation_permission_failure() {
        let symbols=crate::symbol::SymbolOwner::new();let _symbols=symbols.enter();
        let mut graph=InferenceContext::default();let path=graph.atom(Atom::Path).unwrap();let string=graph.atom(Atom::Str).unwrap();
        let why=graph.reason(Span::at(SourceId::new(0),0),None).unwrap();
        let mut values=Vec::new();
        for name in ["first-hash","second-hash"] {
            let arrow=graph.arrow(Arrow {kind:CallableKind::Proc,params:vec![Parameter {label:Name::intern("file"),ty:path,defaulted:false,rest:false}],result:string,effects:EffectSummary::Closed(EffectSet::FS)}).unwrap();
            let scheme=graph.generalize(arrow,0,Generalization::Allowed,&[]).unwrap();
            let candidate=graph.register_candidate(CandidateTemplate {identity:Name::intern(name),public_label:Name::intern(name),scheme,has_receiver:false,actual_eligibility:vec![],argument_relations:vec![ArgumentRelation::Assignable],effect_roles:vec![],output_effect_roles:vec![],failure_projection:None}).unwrap();
            let family=graph.register_family(&[candidate]).unwrap();let instance=graph.instantiate(scheme,1,why).unwrap();values.push(graph.native_callable(candidate,family,instance).unwrap());
        }
        let callback=graph.join_callable_values(values[0],values[1],1,why).unwrap();
        let protocol=graph.arrow(Arrow {kind:CallableKind::Proc,params:vec![Parameter {label:Name::intern("item"),ty:path,defaulted:false,rest:false}],result:string,effects:EffectSummary::Closed(EffectSet::EMPTY)}).unwrap();
        let arrow=graph.arrow(Arrow {kind:CallableKind::Pure,params:vec![Parameter {label:Name::intern("callback"),ty:protocol,defaulted:false,rest:false}],result:string,effects:EffectSummary::Closed(EffectSet::EMPTY)}).unwrap();let scheme=graph.generalize(arrow,0,Generalization::Allowed,&[]).unwrap();
        let candidate=graph.register_candidate(CandidateTemplate {identity:Name::intern("callback-protocol"),public_label:Name::intern("callback-protocol"),scheme,has_receiver:false,actual_eligibility:vec![],argument_relations:vec![ArgumentRelation::InvocationProtocol],effect_roles:vec![],output_effect_roles:vec![],failure_projection:None}).unwrap();let family=graph.register_family(&[candidate]).unwrap();
        let requirement=graph.require_operation(family,OperationCall {binding:OperationBinding::Slots,effect_mode:OperationEffectMode::AvailableBudget,mono_authority:None,declared_error_bound:None,receiver:None,arguments:vec![Some(callback)],result:string,effects:EffectSummary::Closed(EffectSet::EMPTY),effect_bindings:vec![],output_effect_bindings:vec![]},why).unwrap();
        assert!(matches!(graph.solve(),Err(InferenceError::OperationEffectViolation {requirement:source,required:Some(EffectSet::FS),available:Some(EffectSet::EMPTY)}) if source==requirement));
        assert!(graph.requirement_template(requirement).is_ok());assert!(graph.invocation_calls.is_empty());
    }
}

#[cfg(test)]
mod conditional_kind_tests {
    use super::*;
    use crate::source::SourceId;
    #[test]
    fn conditional_protocol_kind_comes_from_every_selected_branch() {
        for (left_kind,right_kind,domain,accepted) in [
            (CallableKind::Pure,CallableKind::Proc,CallableDomain::Exact(CallableKind::Proc),true),
            (CallableKind::Pure,CallableKind::Pure,CallableDomain::Exact(CallableKind::Proc),false),
            (CallableKind::Pure,CallableKind::Proc,CallableDomain::Pure,false),
            (CallableKind::Pure,CallableKind::Stream,CallableDomain::AnyCallable,false),
            (CallableKind::Stream,CallableKind::Stream,CallableDomain::Exact(CallableKind::Stream),true),
        ] {
            let symbols=crate::symbol::SymbolOwner::new();let _symbols=symbols.enter();let mut graph=InferenceContext::default();
            let int=graph.atom(Atom::Int).unwrap();let why=graph.reason(Span::at(SourceId::new(0),0),None).unwrap();let mut arrows=Vec::new();
            for (kind,label) in [(left_kind,"left"),(right_kind,"right")] {arrows.push(graph.arrow(Arrow {kind,params:vec![Parameter {label:Name::intern(label),ty:int,defaulted:false,rest:false}],result:int,effects:EffectSummary::Closed(if kind==CallableKind::Proc {EffectSet::FS}else{EffectSet::EMPTY})}).unwrap());}
            let callable=graph.join_callable_values(arrows[0],arrows[1],0,why).unwrap();let effect=EffectSummary::Closed(if left_kind==CallableKind::Proc||right_kind==CallableKind::Proc {EffectSet::FS}else{EffectSet::EMPTY});
            let requirement=graph.require_callable_invocation(InvocationCall {callable,arguments:vec![InvocationArgument {kind:InvocationArgumentKind::Positional,ty:int}],result:int,effects:effect,domain},why).unwrap();
            let solved=graph.solve();assert_eq!(solved.is_ok(),accepted,"{left_kind:?} {right_kind:?} {domain:?}");
            if accepted {let InvocationPlan::All {branches}=&graph.invocation_evidence(requirement).unwrap().unwrap().plan else {panic!()};assert_eq!(branches.len(),2);assert_eq!(branches.iter().map(|branch|match graph.node(branch.signature).unwrap(){TypeNode::Arrow(arrow)=>arrow.kind,_=>panic!()}).collect::<Vec<_>>(),vec![left_kind,right_kind]);graph.freeze_scoped_with_facts(&[ScopedRoot {ty:callable,scope:None}],&[],&[ScopedRequirementRoot {requirement,scope:None}]).unwrap();}
        }
    }
}
