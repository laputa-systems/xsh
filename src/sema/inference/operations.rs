use super::*;

impl InferenceContext {
    pub fn candidate(&self, id: CandidateId) -> Result<&CandidateTemplate, InferenceError> { slot(&self.candidates, id.index(), id.generation) }
    pub fn family(&self, id: OperationFamilyId) -> Result<&[CandidateId], InferenceError> { Ok(slot(&self.families, id.index(), id.generation)?.as_slice()) }
    pub fn operation_call(&self, id: OperationCallId) -> Result<&OperationCall, InferenceError> { slot(&self.operation_calls, id.index(), id.generation) }
    pub fn register_candidate(&mut self, template: CandidateTemplate) -> Result<CandidateId, InferenceError> {
        self.probe(|graph| {
            graph.constraint()?;
            let scheme = graph.scheme(template.scheme)?;
            if scheme.role != SchemeRole::Value { return Err(InferenceError::InvalidScheme); }
            let body = scheme.body; let effect_roots = scheme.effect_roots.len(); let effect_quantifiers = scheme.effect_quantifiers.len();
            let TypeNode::Arrow(arrow) = graph.clone_node(graph.resolved(body)?)? else { return Err(InferenceError::KindMismatch) };
            graph.work_many(template.argument_relations.len())?;
            for (relation, parameter) in template.argument_relations.iter().zip(&arrow.params) {
                if *relation == ArgumentRelation::InvocationProtocol {
                    let TypeNode::Arrow(protocol) = graph.clone_node(graph.resolved(parameter.ty)?)? else { return Err(InferenceError::InvalidScheme) };
                    graph.work_many(protocol.params.len())?;
                    if protocol.params.iter().any(|parameter| parameter.rest || parameter.defaulted) { return Err(InferenceError::InvalidScheme); }
                }
                let domain = match relation { ArgumentRelation::CommandTarget { domain } => Some((*domain, false)), ArgumentRelation::CommandArgv { element } => Some((*element, true)), _ => None };
                if let Some((domain, argv)) = domain {
                    let mut ty = graph.resolved(parameter.ty)?;
                    if argv { ty = match graph.node(ty)? { TypeNode::List(item) => graph.resolved(*item)?, _ => return Err(InferenceError::InvalidScheme) }; }
                    let expected = match domain { CommandTextDomain::Str => Atom::Str, CommandTextDomain::Path => Atom::Path };
                    if graph.node(ty)? != &TypeNode::Atom(expected) { return Err(InferenceError::InvalidScheme); }
                }
            }
            let offset = usize::from(template.has_receiver);
            if let Some(projection) = template.failure_projection { graph.operation_failure_type(projection, template.has_receiver, body)?; }
            for (_, reference) in &template.effect_roles { if let EffectRoleReference::Fixed(bits) = reference { graph.resolved_effect_summary(EffectSummary::Closed(*bits))?; } }
            if template.output_effect_roles.iter().any(|(_, index)| *index as usize >= effect_roots) || template.output_effect_roles.iter().enumerate().any(|(index, (role, _))| template.output_effect_roles[..index].iter().any(|(prior, _)| prior == role)) { return Err(InferenceError::InvalidScheme); }
            if template.effect_roles.iter().any(|(_, index)| matches!(index, EffectRoleReference::Binder(index) if *index as usize >= effect_quantifiers)) || template.effect_roles.iter().enumerate().any(|(index, (role, _))| template.effect_roles[..index].iter().any(|(prior, _)| prior == role)) || arrow.params.len() < offset || template.actual_eligibility.iter().any(|(index, _)| *index >= arrow.params.len() - offset) { return Err(InferenceError::InvalidScheme); }
            graph.work_many(graph.candidates.len() + template.actual_eligibility.len())?;
            for (index, candidate) in graph.candidates.iter().enumerate() {
                if candidate.value.identity == template.identity {
                    if candidate.value != template { return Err(InferenceError::InvalidScheme); }
                    return Ok(CandidateId { index: index as u32, generation: candidate.generation });
                }
            }
            let id = CandidateId { index: graph.candidates.len() as u32, generation: Self::generation()? };
            graph.candidates.push(Slot { generation: id.generation, value: template }); Ok(id)
        })
    }
    pub fn register_family(&mut self, candidates: &[CandidateId]) -> Result<OperationFamilyId, InferenceError> {
        self.probe(|graph| {
            graph.constraint()?;
            if candidates.is_empty() { return Err(InferenceError::InvalidScheme); }
            let mut sorted = Vec::with_capacity(candidates.len());
            for id in candidates { let candidate = graph.candidate(*id)?; sorted.push((candidate.identity.as_str(), *id)); }
            graph.work_many(candidates.len())?;
            sorted.sort_by(|left, right| left.0.as_str().cmp(right.0.as_str())); sorted.dedup_by_key(|entry| entry.1);
            let candidates: Vec<_> = sorted.into_iter().map(|(_, id)| id).collect();
            for (index, family) in graph.families.iter().enumerate() { if family.value == candidates { return Ok(OperationFamilyId { index: index as u32, generation: family.generation }); } }
            let id = OperationFamilyId { index: graph.families.len() as u32, generation: Self::generation()? };
            graph.families.push(Slot { generation: id.generation, value: candidates }); Ok(id)
        })
    }
    fn store_call(&mut self, mut call: OperationCall) -> Result<OperationCallId, InferenceError> {
        if let OperationBinding::Invocation(invocation) = call.binding { self.invocation_call(invocation)?; if call.receiver.is_some() || !call.arguments.is_empty() || call.effect_mode != OperationEffectMode::ComputedCreation { return Err(InferenceError::InvalidScheme); } }
        if let Some(authority) = call.mono_authority { self.native_authority_signature(authority)?; }
        if let Some(bound) = call.declared_error_bound { self.value_type(bound)?; }
        if let Some(receiver) = call.receiver { self.value_type(receiver)?; }
        for ty in call.arguments.iter().flatten() { self.value_type(*ty)?; }
        self.value_type(call.result)?; self.resolved_effect_summary(call.effects)?; self.work_many(call.arguments.len() + call.effect_bindings.len() + call.output_effect_bindings.len())?;
        call.effect_bindings.sort_by_key(|(role, _)| *role);
        for pair in call.effect_bindings.windows(2) { if pair[0].0 == pair[1].0 { return Err(InferenceError::InvalidScheme); } }
        for (_, effect) in &call.effect_bindings { self.resolved_effect_summary(*effect)?; }
        call.output_effect_bindings.sort_by_key(|(role, _)| *role);
        for pair in call.output_effect_bindings.windows(2) { if pair[0].0 == pair[1].0 { return Err(InferenceError::InvalidScheme); } }
        for (_, effect) in &call.output_effect_bindings { self.resolved_effect_summary(*effect)?; }
        self.constraint()?;
        let id = OperationCallId { index: self.operation_calls.len() as u32, generation: Self::generation()? };
        self.operation_calls.push(Slot { generation: id.generation, value: call }); Ok(id)
    }
    pub fn require_operation(&mut self, family: OperationFamilyId, call: OperationCall, reason: ReasonId) -> Result<RequirementId, InferenceError> {
        self.probe(|graph| { graph.family(family)?; let call = graph.store_call(call)?; graph.new_requirement(RequirementTemplate::Operation { family, call }, reason) })
    }
    pub(super) fn new_requirement(&mut self, template: RequirementTemplate, reason: ReasonId) -> Result<RequirementId, InferenceError> {
        self.reason_data(reason)?; self.constraint()?; self.reason_edge()?;
        let id = RequirementId { index: self.requirements.len() as u32, generation: Self::generation()? };
        self.requirements.push(Slot { generation: id.generation, value: Requirement { source: id, origin: id, template, reason, evidence: None, candidate: None, invocation: None, native_children: Vec::new(), error_join_assignability: None, error_join_output_assignability: None, eligibility: false, queued: false } });
        self.watch_requirement(id)?; self.enqueue(id)?; Ok(id)
    }
    pub fn candidate_evidence(&self, id: RequirementId) -> Result<Option<&CandidateEvidence>, InferenceError> { Ok(if self.requirement(id)?.eligibility { self.requirement(id)?.candidate.as_ref() } else { None }) }
    pub(super) fn solve_operation(&mut self, id: RequirementId, family: OperationFamilyId, call_id: OperationCallId) -> Result<(), InferenceError> {
        if let Some(selected) = self.requirement(id)?.candidate.as_ref() {
            let candidate = selected.candidate;
            let signature = selected.signature;
            self.work_many(selected.dependencies.len())?;
            let dependencies = self.requirement(id)?.candidate.as_ref().unwrap().dependencies.clone();
            let mut complete = self.recheck_candidate_inputs(candidate, signature, call_id, self.requirement(id)?.reason)?;
            let (prepared,_) = self.prepare_candidate_call(candidate,signature,call_id)?;
            complete &= self.validate_candidate_actual_eligibility(candidate, &prepared.arguments)?;
            self.solve_dependencies(&dependencies).map_err(|error| match error { InferenceError::OperationEffectViolation {required,available,..}=>InferenceError::OperationEffectViolation {requirement:id,required,available}, other=>other })?;
            for dependency in dependencies { complete &= self.requirement(dependency)?.eligibility; }
            self.trail_requirement(id)?; self.requirements[id.index()].value.eligibility = complete;
            return Ok(());
        }
        let mut abstract_types = FxHashSet::default();
        for ty in self.requirement_types(self.requirement(id)?.template)? { abstract_types.extend(self.rigid_nodes(ty)?); }
        let mut abstract_effect = false;
        for summary in self.requirement_effects(self.requirement(id)?.template)? { abstract_effect |= matches!(self.resolved_effect_summary(summary)?, EffectSummary::Rigid { .. }); }
        for ty in self.requirement_types(self.requirement(id)?.template)? { abstract_effect |= !self.rigid_effects(ty)?.is_empty(); }
        let abstract_call = !abstract_types.is_empty() || abstract_effect;
        self.work_many(self.family(family)?.len())?;
        let candidates = self.family(family)?.to_vec(); let mut viable = Vec::new(); let mut effect_failure = None; let mut bound_failure = None; let mut invocation_failure = None;
        for candidate in candidates {
            self.counters.candidate_trials += 1;
            match self.trial(|graph| {
                let call = if abstract_call { graph.abstract_trial_call(family, call_id, &abstract_types, graph.requirement(id)?.reason)? } else { call_id };
                graph.apply_candidate(id, candidate, call)
            }) {
                Ok(_) => viable.push(candidate),
                Err(InferenceError::Limit(bound)) => return Err(InferenceError::Limit(bound)),
                Err(error @ InferenceError::OperationErrorBoundViolation { .. }) => { if bound_failure.is_none() { bound_failure = Some(error); } },
                Err(error @ InferenceError::OperationEffectViolation { .. }) => { if effect_failure.is_none() { effect_failure = Some(error); } },
                Err(InferenceError::InvalidInvocation {problem,..}) => { if let OperationBinding::Invocation(call)=self.operation_call(call_id)?.binding { if invocation_failure.is_none() { invocation_failure=Some(InferenceError::InvalidInvocation{call,problem}); } } },
                Err(InferenceError::ForeignHandle | InferenceError::InvalidScheme) => return Err(InferenceError::InvalidScheme),
                Err(_) => {},
            }
        }
        if abstract_call { return if viable.is_empty() { Err(effect_failure.or(bound_failure).or(invocation_failure).unwrap_or(InferenceError::UnsupportedOperation(id))) } else { Ok(()) }; }
        match viable.as_slice() {
            [] => Err(effect_failure.or(bound_failure).or(invocation_failure).unwrap_or(InferenceError::UnsupportedOperation(id))),
            [candidate] => {
                let evidence = self.apply_candidate(id, *candidate, call_id)?;
                self.trail_requirement(id)?; self.requirements[id.index()].value.candidate = Some(evidence);
                Ok(())
            }
            _ => Ok(()),
        }
    }
    fn abstract_trial_call(&mut self, family: OperationFamilyId, call: OperationCallId, rigids: &FxHashSet<TypeId>, reason: ReasonId) -> Result<OperationCallId, InferenceError> {
        let mut replacements = FxHashMap::default(); let mut effects = FxHashMap::default();
        let span = self.reason_data(reason)?.span;
        let mut rigids: Vec<_> = rigids.iter().copied().collect(); rigids.sort(); self.work_many(rigids.len())?;
        for rigid in rigids {
            let TypeNode::Rigid { scope, index, kind } = *self.node(rigid)? else { return Err(InferenceError::InvalidScheme) };
            let quantifier = self.scheme(scope)?.quantifiers.get(index as usize).ok_or(InferenceError::InvalidScheme)?;
            if quantifier.kind != kind { return Err(InferenceError::KindMismatch); }
            self.work_many(quantifier.lacks.len())?;
            let lacks = self.scheme(scope)?.quantifiers[index as usize].lacks.clone();
            let fresh = self.fresh_kind(kind, 1, span)?;
            let TypeNode::Meta(id) = *self.node(fresh)? else { return Err(InferenceError::InvalidScheme) };
            self.metas[id.index()].value.lacks = lacks; replacements.insert(rigid, fresh);
        }
        let template = RequirementTemplate::Operation { family, call };
        let mut summaries = self.requirement_effects(template)?;
        for ty in self.requirement_types(template)? { summaries.extend(self.rigid_effects(ty)?); }
        for summary in summaries {
            let summary = self.resolved_effect_summary(summary)?;
            if let EffectSummary::Rigid { scope, index } = summary {
                if !effects.contains_key(&summary) {
                    let quantifier = self.scheme(scope)?.effect_quantifiers.get(index as usize).copied().ok_or(InferenceError::InvalidScheme)?;
                    let fresh = if quantifier.derived { self.fresh_derived_effect_at(1, quantifier.upper)? } else { self.fresh_effect_at(1, quantifier.upper)? };
                    self.grow_effect(fresh, quantifier.lower)?; effects.insert(summary, EffectSummary::Variable(fresh));
                }
            }
        }
        let mut memo = ReplacementMemo::default();
        let template = self.replace_requirement(template, &replacements, &effects, &mut memo)?;
        let RequirementTemplate::Operation { call, .. } = template else { unreachable!() }; Ok(call)
    }
    fn apply_candidate(&mut self, requirement: RequirementId, candidate_id: CandidateId, call_id: OperationCallId) -> Result<CandidateEvidence, InferenceError> {
        self.work_many(self.candidate(candidate_id)?.actual_eligibility.len() + self.candidate(candidate_id)?.argument_relations.len() + self.operation_call(call_id)?.arguments.len())?;
        let candidate = self.candidate(candidate_id)?.clone(); let mut call = self.operation_call(call_id)?.clone();
        let offset = usize::from(candidate.has_receiver);
        if candidate.has_receiver != call.receiver.is_some() { return Err(InferenceError::KindMismatch); }
        if !candidate.argument_relations.is_empty() && candidate.argument_relations.len() != match self.node(self.resolved(self.scheme(candidate.scheme)?.body)?)? { TypeNode::Arrow(arrow) => arrow.params.len(), _ => return Err(InferenceError::InvalidScheme) } { return Err(InferenceError::InvalidScheme); }
        let mut level = 0;
        for ty in call.receiver.into_iter().chain(call.arguments.iter().filter_map(|ty| *ty)).chain(std::iter::once(call.result)).chain(call.declared_error_bound) {
            for variable in self.free_metas(ty)? { level = level.max(self.meta(variable)?.level); }
            for effect in self.free_effects(ty)? { level = level.max(self.effects[effect.index()].value.level); }
        }
        for summary in std::iter::once(call.effects).chain(call.effect_bindings.iter().map(|(_, summary)| *summary)).chain(call.output_effect_bindings.iter().map(|(_, summary)| *summary)) {
            match self.resolved_effect_summary(summary)? { EffectSummary::Variable(effect) => level = level.max(self.effects[effect.index()].value.level), EffectSummary::Rigid { scope, .. } => level = level.max(self.scheme(scope)?.scope_level), _ => {} }
        }
        let reason = self.requirement(requirement)?.reason;
        let mut instance = if let Some(authority) = call.mono_authority {
            let contract = self.native_authority_member(authority, candidate_id)?;
            let native = self.native_contract(contract)?;
            if native.candidate != candidate_id || native.scheme != candidate.scheme { return Err(InferenceError::KindMismatch); }
            let count = native.instance.requirements.len() + native.instance.requirement_origins.len() + native.instance.substitutions.len() + native.instance.effect_substitutions.len() + native.instance.effect_roots.len(); self.work_many(count)?;
            self.native_contract(contract)?.instance.clone()
        } else { self.instantiate(candidate.scheme, level, reason)? };
        let (prepared, binding) = self.prepare_candidate_call(candidate_id, instance.ty, call_id)?; call = prepared;
        let TypeNode::Arrow(signature) = self.clone_node(self.resolved(instance.ty)?)? else { return Err(InferenceError::InvalidScheme) };
        if signature.params.len() != call.arguments.len() + offset { return Err(InferenceError::KindMismatch); }
        for (parameter, argument) in signature.params[offset..].iter().zip(&call.arguments) { if argument.is_none() && !parameter.defaulted && !parameter.rest { return Err(InferenceError::KindMismatch); } }
        let mut callback_invocations = Vec::new();
        let actuals: Vec<_> = call.receiver.into_iter().map(Some).chain(call.arguments.iter().copied()).collect(); let mut admitted = true;
        for (index, (parameter, actual)) in signature.params.iter().zip(actuals).enumerate() {
            let Some(actual) = actual else { continue };
            let relation = candidate.argument_relations.get(index).copied().unwrap_or(ArgumentRelation::Assignable);
            match relation {
                ArgumentRelation::Assignable => self.assignable(parameter.ty, actual, reason)?,
                ArgumentRelation::Exact => self.unify(parameter.ty, actual, reason)?,
                ArgumentRelation::InvocationProtocol => {
                    let invocation = self.require_invocation_protocol(parameter.ty, actual, reason)?;
                    self.work()?;
                    callback_invocations.push(CallableProtocolEvidence { slot: index, invocation });
                    instance.requirements.push(invocation);
                }
                ArgumentRelation::DeclaredErasure => { admitted &= self.admit_declared_erasure(parameter.ty, actual, reason, false, 0)?; },
                ArgumentRelation::CommandTarget { domain } => { admitted &= self.admit_command_input(domain, false, actual)?; },
                ArgumentRelation::CommandArgv { element } => { admitted &= self.admit_command_input(element, true, actual)?; },
                ArgumentRelation::EqualityCompatible => {
                    instance.requirements.push(self.require_equality_compatible(parameter.ty, actual, reason)?);
                }
            }
        }
        if let OperationBinding::Invocation(invocation)=call.binding { if !self.invocation_branch_kind_allowed(invocation,signature.kind)? { return Err(InferenceError::InvalidInvocation{call:invocation,problem:InvocationProblem::CallableKind}); } }
        let failure_assignability = match (candidate.failure_projection, call.declared_error_bound) {
            (Some(projection), Some(bound)) => {
                let error = self.operation_failure_type(projection, candidate.has_receiver, instance.ty)?;
                let origin = self.origins.len();
                if let Err(failure) = self.assignable(bound, error, reason) {
                    return Err(self.operation_error_bound_failure(requirement, projection, failure)?);
                }
                Some(origin)
            }
            _ => None,
        };
        self.unify(signature.result, call.result, reason)?;
        let mut complete = admitted & self.validate_candidate_actual_eligibility(candidate_id, &call.arguments)?;
        if call.effect_bindings.len() != candidate.effect_roles.len() { return Err(InferenceError::KindMismatch); }
        for (role, ordinal) in &candidate.effect_roles {
            let actual = call.effect_bindings.iter().find(|(actual, _)| actual == role).map(|(_, effect)| *effect).ok_or(InferenceError::KindMismatch)?;
            let expected = match ordinal { EffectRoleReference::Binder(index) => EffectSummary::Variable(*instance.effect_substitutions.get(*index as usize).ok_or(InferenceError::InvalidScheme)?), EffectRoleReference::Fixed(bits) => EffectSummary::Closed(*bits) };
            if let Err(error) = self.unify_effects(expected, actual) { return Err(self.operation_effect_failure(requirement, actual, expected, error)?); }
        }
        if call.output_effect_bindings.len() != candidate.output_effect_roles.len() { return Err(InferenceError::KindMismatch); }
        for (role, root) in &candidate.output_effect_roles {
            let actual = call.output_effect_bindings.iter().find(|(actual, _)| actual == role).map(|(_, effect)| *effect).ok_or(InferenceError::KindMismatch)?;
            let computed = *instance.effect_roots.get(*root as usize).ok_or(InferenceError::InvalidScheme)?;
            if let Err(error) = self.unify_effects(computed, actual) { return Err(self.operation_effect_failure(requirement, computed, actual, error)?); }
        }
        let effect_relation = match call.effect_mode { OperationEffectMode::AvailableBudget => self.include_effects(signature.effects, call.effects, reason), OperationEffectMode::ComputedCreation => self.equate_effects(signature.effects,call.effects,reason) };
        if let Err(error) = effect_relation { return Err(self.operation_effect_failure(requirement, signature.effects, call.effects, error)?); }
        self.solve_dependencies(&instance.requirements).map_err(|error| match error {InferenceError::OperationEffectViolation {required,available,..}=>InferenceError::OperationEffectViolation {requirement,required,available},other=>other})?;
        for dependency in &instance.requirements { complete &= self.requirement(*dependency)?.eligibility; }
        self.trail_requirement(requirement)?; self.requirements[requirement.index()].value.eligibility = complete;
        self.work_many(call.arguments.len())?;
        Ok(CandidateEvidence { callback_invocations, binding, actual_arguments: call.arguments.clone(), candidate: candidate_id, failure_assignability, signature: instance.ty, substitutions: instance.substitutions, effect_substitutions: instance.effect_substitutions, result: call.result, effects: signature.effects, effect_roots: instance.effect_roots, dependencies: instance.requirements })
    }
    pub fn candidate_callback_invocation(&self, requirement: RequirementId, slot: usize) -> Result<Option<RequirementId>, InferenceError> {
        let Some(evidence) = self.candidate_evidence(requirement)? else { return Ok(None) };
        Ok(evidence.callback_invocations.iter().find(|receipt| receipt.slot == slot).map(|receipt| receipt.invocation))
    }
    fn require_invocation_protocol(&mut self, protocol: TypeId, actual: TypeId, reason: ReasonId) -> Result<RequirementId, InferenceError> {
        let TypeNode::Arrow(protocol) = self.clone_node(self.resolved(protocol)?)? else { return Err(InferenceError::InvalidScheme) };
        self.work_many(protocol.params.len())?;
        if protocol.params.iter().any(|parameter| parameter.rest || parameter.defaulted) { return Err(InferenceError::InvalidScheme); }
        let arguments = protocol.params.into_iter().map(|parameter| InvocationArgument { kind: InvocationArgumentKind::Positional, ty: parameter.ty }).collect();
        self.require_callable_invocation(InvocationCall { callable: actual, arguments, result: protocol.result, effects: protocol.effects, domain: CallableDomain::Exact(protocol.kind) }, reason)
    }
    pub(super) fn operation_failure_type(&mut self, projection: OperationFailureProjection, has_receiver: bool, signature: TypeId) -> Result<TypeId, InferenceError> {
        self.work()?;
        let TypeNode::Arrow(signature) = self.node(self.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme) };
        let slot = match projection {
            OperationFailureProjection::ReceiverResultError if has_receiver => 0,
            OperationFailureProjection::ReceiverResultError => return Err(InferenceError::InvalidScheme),
            OperationFailureProjection::ArgumentResultError { argument } => argument.checked_add(usize::from(has_receiver)).ok_or(InferenceError::InvalidScheme)?,
        };
        let parameter = signature.params.get(slot).ok_or(InferenceError::InvalidScheme)?;
        if parameter.defaulted || parameter.rest { return Err(InferenceError::InvalidScheme); }
        match self.node(self.resolved(parameter.ty)?)? {
            TypeNode::Result(_, error) => Ok(*error),
            _ => Err(InferenceError::InvalidScheme),
        }
    }
    fn operation_error_bound_failure(&mut self, requirement: RequirementId, projection: OperationFailureProjection, failure: InferenceError) -> Result<InferenceError, InferenceError> {
        if !matches!(failure, InferenceError::TypeMismatch { .. } | InferenceError::MissingField(_) | InferenceError::Lacks(_) | InferenceError::EffectViolation) { return Ok(failure); }
        self.work()?;
        let RequirementTemplate::Operation { call, .. } = self.requirement(requirement)?.template else { return Err(InferenceError::InvalidScheme) };
        let bound = self.operation_call(call)?.declared_error_bound.ok_or(InferenceError::InvalidScheme)?;
        Ok(InferenceError::OperationErrorBoundViolation { requirement, bound, projection })
    }
    pub(super) fn operation_effect_failure(&mut self, requirement: RequirementId, actual: EffectSummary, expected: EffectSummary, error: InferenceError) -> Result<InferenceError, InferenceError> {
        if error != InferenceError::EffectViolation { return Ok(error); }
        self.work()?;
        let required = match self.resolved_effect_summary(actual)? {
            EffectSummary::Closed(bits) => Some(bits),
            EffectSummary::Variable(id) if self.effects[id.index()].value.bits != EffectSet::EMPTY => Some(self.effects[id.index()].value.bits),
            EffectSummary::Rigid { scope, index } if self.scheme(scope)?.effect_quantifiers[index as usize].lower != EffectSet::EMPTY => Some(self.scheme(scope)?.effect_quantifiers[index as usize].lower),
            _ => None,
        };
        let available = match self.resolved_effect_summary(expected)? {
            EffectSummary::Closed(bits) => Some(bits),
            EffectSummary::Variable(id) => self.effects[id.index()].value.upper,
            EffectSummary::Rigid { scope, index } => self.scheme(scope)?.effect_quantifiers[index as usize].upper,
            EffectSummary::Unknown => None,
        };
        Ok(InferenceError::OperationEffectViolation { requirement, required, available })
    }
    pub(super) fn solve_dependencies(&mut self, requirements: &[RequirementId]) -> Result<(), InferenceError> {
        for requirement in requirements {
            match self.requirement(*requirement)?.template {
                RequirementTemplate::ErrorJoin {join} => self.solve_error_join(*requirement,join)?,
                RequirementTemplate::EffectInclusion { actual, expected, excluded } => self.solve_masked_effect_inclusion(*requirement, actual, expected, excluded)?,
                RequirementTemplate::Eligibility { predicate, ty } => self.solve_eligibility(*requirement, predicate, ty)?,
                RequirementTemplate::EqualityCompatible { left, right } => self.solve_equality_compatible(*requirement, left, right)?,
                RequirementTemplate::CallableInvocation { call } => self.solve_callable_invocation(*requirement, call)?,
                RequirementTemplate::Add { .. } => return Err(InferenceError::InvalidScheme),
                RequirementTemplate::Operation { .. } => return Err(InferenceError::InvalidScheme),
            }
        }
        Ok(())
    }
    pub(super) fn prepare_candidate_call(&mut self, candidate: CandidateId, signature: TypeId, call: OperationCallId) -> Result<(OperationCall, Option<InvocationBinding>), InferenceError> {
        self.work_many(self.operation_call(call)?.arguments.len() + self.operation_call(call)?.output_effect_bindings.len() + self.operation_call(call)?.effect_bindings.len())?;
        let mut prepared = self.operation_call(call)?.clone();
        let OperationBinding::Invocation(invocation) = prepared.binding else { return Ok((prepared,None)) };
        if prepared.receiver.is_some() || !prepared.arguments.is_empty() || prepared.mono_authority.is_none() || self.candidate(candidate)?.has_receiver { return Err(InferenceError::InvalidScheme); }
        self.work_many(self.invocation_call(invocation)?.arguments.len())?; let original = self.invocation_call(invocation)?.clone();
        let TypeNode::Arrow(arrow) = self.clone_node(self.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme) };
        let kinds = original.arguments.iter().map(|argument| argument.kind).collect::<Vec<_>>();
        let binding = self.plan_invocation_arguments(signature,&kinds).map_err(|error| match error { InvocationPlanError::Graph(error) => error, InvocationPlanError::Binding(problem) => InferenceError::InvalidInvocation { call: invocation,problem } })?;
        if binding.dynamic.is_some() || binding.rest_slot.is_some() { return Err(InferenceError::Boundary("native family needs a fixed supplied argument plan")); }
        self.work_many(arrow.params.len() + self.candidate(candidate)?.output_effect_roles.len())?;
        prepared.arguments = vec![None;arrow.params.len()];
        for (argument,slot) in original.arguments.iter().zip(&binding.supplied_slots) { prepared.arguments[*slot]=Some(argument.ty); }
        let authority = prepared.mono_authority.unwrap(); let member = self.native_authority_member(authority,candidate)?;
        if prepared.output_effect_bindings.is_empty() {
            let outputs=self.candidate(candidate)?.output_effect_roles.clone();
            for (role,index) in outputs { prepared.output_effect_bindings.push((role,*self.native_contract(member)?.instance.effect_roots.get(index as usize).ok_or(InferenceError::InvalidScheme)?)); }
        }
        Ok((prepared,Some(binding)))
    }
    pub fn candidate_output_effect(&self, requirement: RequirementId, role: ProducerRole) -> Result<Option<EffectSummary>, InferenceError> {
        let Some(evidence) = self.candidate_evidence(requirement)? else { return Ok(None) };
        let Some((_,index)) = self.candidate(evidence.candidate)?.output_effect_roles.iter().find(|(actual,_)| *actual==role) else { return Ok(None) };
        Ok(Some(*evidence.effect_roots.get(*index as usize).ok_or(InferenceError::InvalidScheme)?))
    }
    fn validate_candidate_actual_eligibility(&mut self, candidate: CandidateId, actuals: &[Option<TypeId>]) -> Result<bool, InferenceError> {
        self.work_many(self.candidate(candidate)?.actual_eligibility.len())?; let predicates=self.candidate(candidate)?.actual_eligibility.clone(); let mut complete=true;
        for (slot,predicate) in predicates { if let Some(actual)=actuals.get(slot).copied().flatten() { complete &= self.eligibility_state(predicate,actual)?; } } Ok(complete)
    }
    fn recheck_candidate_inputs(&mut self, candidate: CandidateId, signature: TypeId, call: OperationCallId, reason: ReasonId) -> Result<bool, InferenceError> {
        self.work_many(self.candidate(candidate)?.argument_relations.len() + self.operation_call(call)?.arguments.len())?;
        let relations = self.candidate(candidate)?.argument_relations.clone();
        let (prepared,_) = self.prepare_candidate_call(candidate,signature,call)?;
        let receiver = prepared.receiver; let arguments = prepared.arguments;
        let TypeNode::Arrow(signature) = self.clone_node(self.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme) };
        let actuals = receiver.into_iter().map(Some).chain(arguments); let mut complete = true;
        for (index, (parameter, actual)) in signature.params.iter().zip(actuals).enumerate() {
            self.work()?;
            if let Some(actual) = actual {
                match relations.get(index) {
                    Some(ArgumentRelation::DeclaredErasure) => complete &= self.admit_declared_erasure(parameter.ty, actual, reason, false, 0)?,
                    Some(ArgumentRelation::CommandTarget { domain }) => complete &= self.admit_command_input(*domain, false, actual)?,
                    Some(ArgumentRelation::CommandArgv { element }) => complete &= self.admit_command_input(*element, true, actual)?,
                    _ => {},
                }
            }
        }
        Ok(complete)
    }
    fn admit_command_input(&mut self, domain: CommandTextDomain, argv: bool, actual: TypeId) -> Result<bool, InferenceError> {
        self.work()?; let actual = self.resolved(actual)?;
        let node = self.clone_node(actual)?;
        if matches!(node, TypeNode::Meta(_) | TypeNode::Rigid { .. }) { return Ok(false); }
        let item = if argv { match node {
            TypeNode::List(item) => self.clone_node(self.resolved(item)?)?,
            TypeNode::Atom(Atom::Any) => TypeNode::Atom(Atom::Any),
            _ => return Err(InferenceError::Boundary("native command argv must retain its checked list domain")),
        } } else { node };
        match item {
            TypeNode::Meta(_) | TypeNode::Rigid { .. } => Ok(false),
            TypeNode::Atom(Atom::Str) if domain == CommandTextDomain::Str => Ok(true),
            TypeNode::Atom(Atom::Path) if domain == CommandTextDomain::Path => Ok(true),
            TypeNode::Atom(Atom::Any) if domain == CommandTextDomain::Str => Ok(true),
            _ => Err(InferenceError::Boundary("native command input does not match this text domain")),
        }
    }
    fn admit_declared_erasure(&mut self, expected: TypeId, actual: TypeId, reason: ReasonId, invariant: bool, depth: usize) -> Result<bool, InferenceError> {
        self.work()?; if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
        let expected = self.resolved(expected)?; let actual = self.resolved(actual)?;
        match (self.clone_node(expected)?, self.clone_node(actual)?) {
            (TypeNode::Atom(Atom::Any), _) => { self.value_type(actual)?; Ok(true) },
            (TypeNode::Atom(Atom::ErasedRecord), TypeNode::Record(_)) => Ok(true),
            (_, TypeNode::Meta(_) | TypeNode::Rigid { .. }) => Ok(false),
            (TypeNode::List(a), TypeNode::List(b)) | (TypeNode::Stream(a), TypeNode::Stream(b)) | (TypeNode::Optional(a), TypeNode::Optional(b)) => self.admit_declared_erasure(a, b, reason, true, depth + 1),
            (TypeNode::Map(a, b), TypeNode::Map(c, d)) | (TypeNode::Result(a, b), TypeNode::Result(c, d)) => {
                let first = self.admit_declared_erasure(a, c, reason, true, depth + 1)?; Ok(first & self.admit_declared_erasure(b, d, reason, true, depth + 1)?)
            }
            (TypeNode::Record(a), TypeNode::Record(b)) if invariant => {
                let a = self.clone_row(a)?; let b = self.clone_row(b)?;
                if a.fields.len() != b.fields.len() || a.tail != b.tail { return Err(InferenceError::TypeMismatch { left: expected, right: actual }); }
                let mut complete = true;
                for (a, b) in a.fields.iter().zip(&b.fields) {
                    if a.label != b.label { return Err(InferenceError::TypeMismatch { left: expected, right: actual }); }
                    complete &= self.admit_declared_erasure(a.ty, b.ty, reason, true, depth + 1)?;
                }
                Ok(complete)
            }
            _ if invariant => { self.unify(expected, actual, reason)?; Ok(true) },
            _ => { self.assignable(expected, actual, reason)?; Ok(true) },
        }
    }
    /// A certificate for a concrete source use stays in the graph, while only
    /// obligations depending on declaration binders belong in its callable scheme.
    pub(super) fn requirement_is_residual(&mut self, id: RequirementId) -> Result<bool, InferenceError> {
        self.work()?;
        if let RequirementTemplate::EffectInclusion { actual, expected, excluded } = self.requirement(id)?.template { if self.masked_effect_tautology(actual, expected, excluded)? { return Ok(false); } }
        let requirement = self.requirement(id)?;
        let certified = match requirement.template {
            RequirementTemplate::ErrorJoin {..} => requirement.eligibility,
            RequirementTemplate::Add { .. } => requirement.evidence.is_some(),
            RequirementTemplate::Eligibility { .. } | RequirementTemplate::EqualityCompatible { .. } | RequirementTemplate::EffectInclusion { .. } => requirement.eligibility,
            RequirementTemplate::Operation { .. } => requirement.candidate.is_some() && requirement.eligibility,
            RequirementTemplate::CallableInvocation { .. } => requirement.invocation.is_some() && requirement.eligibility,
        };
        if !certified { return Ok(true); }
        let template = requirement.template;
        let mut pending: Vec<_> = self.requirement_types(template)?.into_iter().map(|ty| (ty, 0usize)).collect();
        let mut seen = FxHashSet::default();
        while let Some((ty, depth)) = pending.pop() {
            self.work()?; if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
            let ty = self.resolved(ty)?; if !seen.insert(ty) { continue; }
            match self.node(ty)? {
                TypeNode::Meta(_) | TypeNode::Rigid { .. } => return Ok(true),
                TypeNode::Poison => return Err(InferenceError::Recovery(ty)),
                TypeNode::Arrow(arrow) if matches!(self.resolved_effect_summary(arrow.effects)?, EffectSummary::Variable(_) | EffectSummary::Rigid { .. }) => return Ok(true),
                _ => {},
            }
            for child in self.children(ty)? { pending.push((child, depth + 1)); }
        }
        if let RequirementTemplate::Operation { call, .. } = template {
            for summary in self.operation_call(call)?.effect_bindings.iter().map(|(_, summary)| summary).chain(self.operation_call(call)?.output_effect_bindings.iter().map(|(_, summary)| summary)) { if matches!(self.resolved_effect_summary(*summary)?, EffectSummary::Variable(_) | EffectSummary::Rigid { .. }) { return Ok(true); } }
            if let Some(evidence) = &self.requirement(id)?.candidate { if matches!(self.resolved_effect_summary(evidence.effects)?, EffectSummary::Variable(_) | EffectSummary::Rigid { .. }) { return Ok(true); } }
        }
        if let RequirementTemplate::CallableInvocation { call } = template { if matches!(self.resolved_effect_summary(self.invocation_call(call)?.effects)?, EffectSummary::Variable(_) | EffectSummary::Rigid { .. }) { return Ok(true); } }
        if let RequirementTemplate::EffectInclusion { actual, expected, .. } = template { if matches!(self.resolved_effect_summary(actual)?, EffectSummary::Variable(_) | EffectSummary::Rigid { .. }) || matches!(self.resolved_effect_summary(expected)?, EffectSummary::Variable(_) | EffectSummary::Rigid { .. }) { return Ok(true); } }
        Ok(false)
    }
    /// A selected operation can calculate a summary outside an authored closed
    /// caller header. Its semantic outputs must close before deciding whether
    /// the source obligation still depends on declaration binders.
    pub(super) fn requirement_effect_roots(&mut self, id: RequirementId) -> Result<Vec<EffectSummary>, InferenceError> {
        let template = self.requirement(id)?.template;
        let payload = match template {
            RequirementTemplate::Operation { call, .. } => { let call = self.operation_call(call)?; 1 + call.effect_bindings.len() + call.output_effect_bindings.len() },
            RequirementTemplate::CallableInvocation { .. } => 1,
            RequirementTemplate::EffectInclusion { .. } => 2,
            _ => 0,
        };
        let certificate = self.requirement(id)?.candidate.as_ref().map_or(0, |evidence| 1 + evidence.effect_roots.len());
        self.work_many(payload + certificate)?;
        let mut roots = self.requirement_effects(template)?;
        if let Some(evidence) = self.requirement(id)?.candidate.as_ref() { roots.push(evidence.effects); roots.extend_from_slice(&evidence.effect_roots); }
        Ok(roots)
    }
    pub(super) fn requirement_effects(&self, template: RequirementTemplate) -> Result<Vec<EffectSummary>, InferenceError> {
        Ok(match template {
            RequirementTemplate::EffectInclusion { actual, expected, .. } => vec![actual, expected],
            RequirementTemplate::Operation { call, .. } => { let call = self.operation_call(call)?; std::iter::once(call.effects).chain(call.effect_bindings.iter().map(|(_, summary)| *summary)).chain(call.output_effect_bindings.iter().map(|(_, summary)| *summary)).collect() },
            RequirementTemplate::CallableInvocation { call } => vec![self.invocation_call(call)?.effects],
            _ => Vec::new(),
        })
    }
    pub(super) fn pending_operation_effect(&mut self, effect: EffectId) -> Result<bool, InferenceError> {
        self.work_many(self.effects[effect.index()].value.watchers.len())?;
        let watchers = self.effects[effect.index()].value.watchers.clone(); let mut pending = false;
        for requirement in watchers {
            self.work()?; let state = self.requirement(requirement)?;
            if let RequirementTemplate::EffectInclusion { actual, expected, excluded } = state.template {
                if !self.masked_effect_tautology(actual, expected, excluded)? && self.resolved_effect_summary(expected)? == EffectSummary::Variable(effect) && matches!(self.resolved_effect_summary(actual)?, EffectSummary::Variable(_) | EffectSummary::Rigid { .. }) { pending = true; }
                continue;
            }
            if let RequirementTemplate::CallableInvocation { call } = state.template {
                if state.invocation.is_none() {
                    let call = self.invocation_call(call)?;
                    if self.resolved_effect_summary(call.effects)? == EffectSummary::Variable(effect) {
                        if matches!(call.domain, CallableDomain::Pure | CallableDomain::Exact(CallableKind::Pure)) { self.include_effects(EffectSummary::Closed(EffectSet::EMPTY), EffectSummary::Variable(effect), self.requirement(requirement)?.reason)?; }
                        else { pending = true; }
                    }
                }
                continue;
            }
            let RequirementTemplate::Operation { family, call } = state.template else { continue };
            if state.candidate.is_some() { continue; }
            let call = self.operation_call(call)?;
            let creation = self.resolved_effect_summary(call.effects)? == EffectSummary::Variable(effect);
            let outputs: Vec<_> = call.output_effect_bindings.iter().filter_map(|(role, summary)| match self.resolved_effect_summary(*summary) { Ok(EffectSummary::Variable(id)) if id == effect => Some(*role), _ => None }).collect();
            if !creation && outputs.is_empty() { continue; }
            self.work_many(self.family(family)?.len())?; let candidates = self.family(family)?.to_vec();
            let mut common = None; let mut finite = true;
            for candidate in candidates {
                self.work()?; let candidate = self.candidate(candidate)?; let scheme = self.scheme(candidate.scheme)?;
                let TypeNode::Arrow(signature) = self.node(self.resolved(scheme.body)?)? else { return Err(InferenceError::InvalidScheme) };
                let mut summaries = Vec::new();
                if creation { summaries.push(signature.effects); }
                for output in &outputs {
                    let root = candidate.output_effect_roles.iter().find(|(role, _)| role == output).map(|(_, root)| *root).ok_or(InferenceError::InvalidScheme)?;
                    summaries.push(*scheme.effect_roots.get(root as usize).ok_or(InferenceError::InvalidScheme)?);
                }
                self.work_many(summaries.len())?;
                for summary in summaries {
                    let summary = self.resolved_effect_summary(summary)?;
                    if !matches!(summary, EffectSummary::Closed(_) | EffectSummary::Unknown) { finite = false; }
                    if let Some(previous) = common { if previous != summary { finite = false; } } else { common = Some(summary); }
                }
            }
            if finite { if let Some(summary) = common { self.include_effects(summary, EffectSummary::Variable(effect), self.requirement(requirement)?.reason)?; } }
            else { pending = true; }
        }
        Ok(pending)
    }
    pub(super) fn requirement_types(&self, template: RequirementTemplate) -> Result<Vec<TypeId>, InferenceError> {
        Ok(match template {
            RequirementTemplate::ErrorJoin {join} => {let join=self.error_join(join)?;join.inputs.iter().copied().chain(std::iter::once(join.result)).chain(join.bound).collect()},
            RequirementTemplate::EffectInclusion { .. } => Vec::new(),
            RequirementTemplate::Add { left, right, result } => vec![left, right, result],
            RequirementTemplate::Eligibility { ty, .. } => vec![ty],
            RequirementTemplate::EqualityCompatible { left, right } => vec![left, right],
            RequirementTemplate::Operation { call, .. } => { let call = self.operation_call(call)?; call.receiver.into_iter().chain(call.arguments.iter().filter_map(|ty| *ty)).chain(match call.binding { OperationBinding::Invocation(id) => self.invocation_call(id)?.arguments.iter().map(|argument| argument.ty).collect::<Vec<_>>(), OperationBinding::Slots => Vec::new() }).chain(std::iter::once(call.result)).chain(call.declared_error_bound).collect() }
            RequirementTemplate::CallableInvocation { call } => { let call = self.invocation_call(call)?; std::iter::once(call.callable).chain(call.arguments.iter().map(|argument| argument.ty)).chain(std::iter::once(call.result)).collect() }
        })
    }
    pub(super) fn replace_requirement(&mut self, template: RequirementTemplate, types: &FxHashMap<TypeId, TypeId>, effects: &FxHashMap<EffectSummary, EffectSummary>, memo: &mut ReplacementMemo) -> Result<RequirementTemplate, InferenceError> {
        Ok(match template {
            RequirementTemplate::ErrorJoin {join} => RequirementTemplate::ErrorJoin {join:self.replace_error_join(join,types,effects,memo)?},
            RequirementTemplate::EffectInclusion { actual, expected, excluded } => { let actual = self.resolved_effect_summary(actual)?; let expected = self.resolved_effect_summary(expected)?; RequirementTemplate::EffectInclusion { actual: *effects.get(&actual).unwrap_or(&actual), expected: *effects.get(&expected).unwrap_or(&expected), excluded } },
            RequirementTemplate::Add { left, right, result } => RequirementTemplate::Add { left: self.replace(left, types, effects, memo, 0)?, right: self.replace(right, types, effects, memo, 0)?, result: self.replace(result, types, effects, memo, 0)? },
            RequirementTemplate::Eligibility { predicate, ty } => RequirementTemplate::Eligibility { predicate, ty: self.replace(ty, types, effects, memo, 0)? },
            RequirementTemplate::EqualityCompatible { left, right } => RequirementTemplate::EqualityCompatible { left: self.replace(left, types, effects, memo, 0)?, right: self.replace(right, types, effects, memo, 0)? },
            RequirementTemplate::CallableInvocation { call } => RequirementTemplate::CallableInvocation { call: self.replace_invocation(call, types, effects, memo)? },
            RequirementTemplate::Operation { family, call } => {
                self.work_many(self.operation_call(call)?.arguments.len() + self.operation_call(call)?.effect_bindings.len() + self.operation_call(call)?.output_effect_bindings.len())?; let call = self.operation_call(call)?.clone();
                let receiver = call.receiver.map(|ty| self.replace(ty, types, effects, memo, 0)).transpose()?;
                let arguments = call.arguments.into_iter().map(|ty| ty.map(|ty| self.replace(ty, types, effects, memo, 0)).transpose()).collect::<Result<_, _>>()?;
                let declared_error_bound = call.declared_error_bound.map(|ty| self.replace(ty, types, effects, memo, 0)).transpose()?;
                let result = self.replace(call.result, types, effects, memo, 0)?; let summary = self.resolved_effect_summary(call.effects)?;
                let effect_bindings = call.effect_bindings.into_iter().map(|(role, summary)| { let summary = self.resolved_effect_summary(summary)?; Ok((role, *effects.get(&summary).unwrap_or(&summary))) }).collect::<Result<Vec<_>, InferenceError>>()?;
                let output_effect_bindings = call.output_effect_bindings.into_iter().map(|(role, summary)| { let summary = self.resolved_effect_summary(summary)?; Ok((role, *effects.get(&summary).unwrap_or(&summary))) }).collect::<Result<Vec<_>, InferenceError>>()?;
                let mono_authority = call.mono_authority.map(|authority| self.replace_native_authority(authority, types, effects, memo, 0)).transpose()?;
                let binding = match call.binding { OperationBinding::Slots => OperationBinding::Slots, OperationBinding::Invocation(id) => OperationBinding::Invocation(self.replace_invocation(id,types,effects,memo)?) };
                let call = self.store_call(OperationCall { binding, effect_mode: call.effect_mode, mono_authority, declared_error_bound, effect_bindings, output_effect_bindings, receiver, arguments, result, effects: *effects.get(&summary).unwrap_or(&summary) })?;
                RequirementTemplate::Operation { family, call }
            }
        })
    }
    pub(super) fn instantiate_requirement(&mut self, template: RequirementTemplate, reason: ReasonId) -> Result<RequirementId, InferenceError> {
        match template {
            RequirementTemplate::Add { left, right, result } => self.require_add(left, right, result, reason),
            RequirementTemplate::CallableInvocation { call } => { self.contribute(ConstraintRelation::CallableInvocation { call }, reason)?; self.new_requirement(template, reason) },
            _ => self.new_requirement(template, reason),
        }
    }
}
