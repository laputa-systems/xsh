use super::*;

impl InferenceContext {
    pub fn invocation_call(&self, id: InvocationCallId) -> Result<&InvocationCall, InferenceError> { slot(&self.invocation_calls, id.index(), id.generation) }
    pub fn invocation_evidence(&self, id: RequirementId) -> Result<Option<&InvocationEvidence>, InferenceError> { Ok(self.requirement(id)?.invocation.as_ref()) }
    pub fn require_callable_invocation(&mut self, call: InvocationCall, reason: ReasonId) -> Result<RequirementId, InferenceError> {
        self.probe(|graph| {
            let call = graph.store_invocation(call)?;
            graph.contribute(ConstraintRelation::CallableInvocation { call }, reason)?;
            graph.new_requirement(RequirementTemplate::CallableInvocation { call }, reason)
        })
    }
    fn store_invocation(&mut self, call: InvocationCall) -> Result<InvocationCallId, InferenceError> {
        self.value_type(call.callable)?; self.value_type(call.result)?; self.resolved_effect_summary(call.effects)?;
        self.work_many(call.arguments.len())?;
        for argument in &call.arguments { self.value_type(argument.ty)?; }
        self.constraint()?;
        let id = InvocationCallId { index: self.invocation_calls.len() as u32, generation: Self::generation()? };
        self.invocation_calls.push(Slot { generation: id.generation, value: call }); Ok(id)
    }
    pub(super) fn replace_invocation(&mut self, id: InvocationCallId, types: &FxHashMap<TypeId, TypeId>, effects: &FxHashMap<EffectSummary, EffectSummary>, memo: &mut ReplacementMemo) -> Result<InvocationCallId, InferenceError> {
        self.work()?;
        if let Some(replacement) = memo.invocations.get(&id) { return Ok(*replacement); }
        self.work_many(self.invocation_call(id)?.arguments.len())?; let call = self.invocation_call(id)?.clone();
        let callable = self.replace(call.callable, types, effects, memo, 0)?;
        let result = self.replace(call.result, types, effects, memo, 0)?;
        let arguments = call.arguments.into_iter().map(|argument| Ok(InvocationArgument { kind: argument.kind, ty: self.replace(argument.ty, types, effects, memo, 0)? })).collect::<Result<_, InferenceError>>()?;
        let summary = self.resolved_effect_summary(call.effects)?;
        let replacement = self.store_invocation(InvocationCall { callable, arguments, result, domain: call.domain, effects: *effects.get(&summary).unwrap_or(&summary) })?;
        memo.invocations.insert(id, replacement); Ok(replacement)
    }
    // Child obligations belong to an immediate checked invocation. Replaying
    // them preserves that identity even when two outer bodies use one native
    // reference; call and authority replacement share the same instance memo.
    pub(super) fn replace_native_invocation_children(&mut self, source: RequirementId, target: RequirementId, types: &FxHashMap<TypeId, TypeId>, effects: &FxHashMap<EffectSummary, EffectSummary>, memo: &mut ReplacementMemo, depth: usize) -> Result<(), InferenceError> {
        self.work()?;
        if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
        self.work_many(self.native_invocation_children(source)?.len())?;
        let children = self.native_invocation_children(source)?.to_vec();
        let target_authorities = if !children.is_empty() {
            let RequirementTemplate::CallableInvocation { call: source_call } = self.requirement(source)?.template else { return Err(InferenceError::InvalidScheme) };
            let RequirementTemplate::CallableInvocation { call: target_call } = self.requirement(target)?.template else { return Err(InferenceError::InvalidScheme) };
            memo.invocations.insert(source_call, target_call);
            self.work_many(self.invocation_call(source_call)?.arguments.len() + self.invocation_call(target_call)?.arguments.len())?;
            let original = self.invocation_call(source_call)?.clone(); let replaced = self.invocation_call(target_call)?.clone();
            if original.arguments.len() != replaced.arguments.len() || original.domain != replaced.domain { return Err(InferenceError::InvalidScheme); }
            for (source, target) in original.arguments.iter().zip(&replaced.arguments) {
                if source.kind != target.kind { return Err(InferenceError::InvalidScheme); }
                memo.types.insert(self.resolved(source.ty)?, target.ty);
            }
            memo.types.insert(self.resolved(original.result)?, replaced.result);
            memo.types.insert(self.resolved(original.callable)?, replaced.callable);
            let TypeNode::NativeCallable(callable) = self.clone_node(self.resolved(replaced.callable)?)? else { return Err(InferenceError::InvalidScheme) };
            callable.alternatives
        } else { Vec::new() };
        for child in children {
            self.work_many(target_authorities.len() + 1)?;
            let origin = self.native_authority_origin(child.authority)?;
            let mut authority = None;
            for target in &target_authorities {
                if let CallableAuthority::Native { authority: target } = target {
                    if self.native_authority_origin(*target)? == origin { authority = Some(*target); break; }
                }
            }
            let authority = authority.ok_or(InferenceError::InvalidScheme)?;
            match (child.authority, authority) {
                (NativeAuthority::Single(source), NativeAuthority::Single(target)) => { memo.contracts.insert(source, target); },
                (NativeAuthority::Family(source), NativeAuthority::Family(target)) => { memo.families.insert(source, target); },
                _ => return Err(InferenceError::InvalidScheme),
            }
            let operation = if let Some(replacement) = memo.requirements.get(&child.operation) { *replacement } else {
                let original = self.requirement(child.operation)?; let template = original.template; let reason = original.reason; let origin = original.origin;
                let template = self.replace_requirement(template, types, effects, memo)?;
                let replacement = self.instantiate_requirement(template, reason)?;
                self.requirements[replacement.index()].value.source = child.operation;
                self.requirements[replacement.index()].value.origin = origin;
                memo.requirements.insert(child.operation, replacement);
                self.replace_native_invocation_children(child.operation, replacement, types, effects, memo, depth + 1)?;
                replacement
            };
            self.work()?; self.trail_requirement(target)?;
            self.requirements[target.index()].value.native_children.push(NativeInvocationAlternative { authority, operation });
        }
        Ok(())
    }
    pub(super) fn solve_callable_invocation(&mut self, id: RequirementId, call_id: InvocationCallId) -> Result<(), InferenceError> {
        self.work_many(self.invocation_call(call_id)?.arguments.len())?; let call = self.invocation_call(call_id)?.clone();
        let reason = self.requirement(id)?.reason;
        if matches!(self.node(self.resolved(call.callable)?)?, TypeNode::NativeCallable(callable) if callable.alternatives.len() > 1) { return self.solve_callable_all(id, call_id); }
        let native_only = matches!(self.node(self.resolved(call.callable)?)?, TypeNode::NativeCallable(callable) if callable.alternatives.iter().all(|authority| matches!(authority, CallableAuthority::Native { .. })));
        if matches!(call.domain, CallableDomain::Pure | CallableDomain::Exact(CallableKind::Pure)) { self.unify_effects(call.effects, EffectSummary::Closed(EffectSet::EMPTY))?; }
        let original = self.resolved(call.callable)?;
        let signature = match self.node(original)? { TypeNode::NativeCallable(_) => self.resolved(self.callable_signature(original)?)?, _ => original };
        let arrow = match self.clone_node(signature)? {
            TypeNode::Meta(_) | TypeNode::Rigid { .. } => return Ok(()),
            TypeNode::Arrow(arrow) => arrow,
            TypeNode::CallableChoice(_) => return self.solve_callable_choice(id,call_id),
            TypeNode::Poison | TypeNode::NonCompletion => return Err(InferenceError::Recovery(signature)),
            _ => return Err(InferenceError::InvalidInvocation { call: call_id, problem: InvocationProblem::NotCallable }),
        };
        let fail = |problem| InferenceError::InvalidInvocation { call: call_id, problem };
        if !call.domain.admits(arrow.kind) { return Err(fail(InvocationProblem::CallableKind)); }
        if arrow.kind == CallableKind::Pure { self.include_effects(arrow.effects, EffectSummary::Closed(EffectSet::EMPTY), reason)?; }
        self.work_many(call.arguments.len())?;
        let kinds: Vec<_> = call.arguments.iter().map(|argument| argument.kind).collect();
        let binding = self.plan_invocation_arguments(signature, &kinds).map_err(|error| match error { InvocationPlanError::Graph(error) => error, InvocationPlanError::Binding(problem) => fail(problem) })?;
        if !self.admit_invocation_binding(&arrow,&call.arguments,&binding,call_id,reason,native_only)? { return Ok(()); }
        self.unify(call.result, arrow.result, reason)?; self.unify_effects(call.effects, arrow.effects)?;
        let native_alternatives = self.native_invocation_operations(id, call.callable, &call.arguments, &binding)?;
        let mut complete = true; for alternative in &native_alternatives { self.work()?; complete &= self.requirement(alternative.operation)?.eligibility; }
        self.work_many(native_alternatives.len())?; self.trail_requirement(id)?;
        self.requirements[id.index()].value.native_children = native_alternatives.clone();
        if native_only && native_alternatives.iter().any(|alternative| self.requirement(alternative.operation).is_ok_and(|state| state.candidate.is_none())) {
            self.requirements[id.index()].value.invocation = None; self.requirements[id.index()].value.eligibility = false; return Ok(());
        }
        let (selected_signature,selected_binding) = if native_only && native_alternatives.len()==1 {
            if let Some(selected)=&self.requirement(native_alternatives[0].operation)?.candidate {
                let signature=selected.signature; self.work_many(selected.binding.as_ref().map_or(0,|binding|binding.supplied_slots.len()+binding.default_slots.len()))?;
                let binding=self.requirement(native_alternatives[0].operation)?.candidate.as_ref().unwrap().binding.clone().unwrap_or(binding);
                (signature,binding)
            } else {(signature,binding)}
        } else {(signature,binding)};
        self.trail_requirement(id)?;
        self.requirements[id.index()].value.invocation = Some(InvocationEvidence { callable: call.callable, native_alternatives, plan: InvocationPlan::Unique { signature:selected_signature, binding:selected_binding, timing: if arrow.kind == CallableKind::Stream { InvocationDefaultTiming::AtPull } else { InvocationDefaultTiming::AtCall } }, result: call.result, effects: arrow.effects });
        self.requirements[id.index()].value.eligibility = complete;
        Ok(())
    }
    pub fn native_invocation_children(&self, requirement: RequirementId) -> Result<&[NativeInvocationAlternative], InferenceError> { let state=self.requirement(requirement)?; if state.native_children.is_empty() { if let Some(evidence)=&state.invocation { return Ok(&evidence.native_alternatives); } } Ok(&state.native_children) }
    fn solve_callable_choice(&mut self, requirement: RequirementId, invocation: InvocationCallId) -> Result<(), InferenceError> {
        self.work_many(self.invocation_call(invocation)?.arguments.len())?; let original=self.invocation_call(invocation)?.clone();
        let TypeNode::NativeCallable(callable)=self.clone_node(self.resolved(original.callable)?)? else { return Err(InferenceError::InvalidScheme) };
        let [CallableAuthority::Native { authority: NativeAuthority::Family(family) }]=callable.alternatives.as_slice() else { return Err(InferenceError::Boundary("conditional callable choices need separate branch binding proofs")); }; let authority=NativeAuthority::Family(*family);
        let reason=self.requirement(requirement)?.reason;
        let child=if let Some(child)=self.requirement(requirement)?.native_children.first() { if child.authority!=authority { return Err(InferenceError::InvalidScheme); } child.operation } else {
            let family=self.native_family_contract(*family)?.family;
            let child=self.require_operation(family,OperationCall {binding:OperationBinding::Invocation(invocation),effect_mode:OperationEffectMode::ComputedCreation,mono_authority:Some(authority),declared_error_bound:None,receiver:None,arguments:Vec::new(),result:original.result,effects:original.effects,effect_bindings:Vec::new(),output_effect_bindings:Vec::new()},reason)?;
            self.work()?; self.trail_requirement(requirement)?; self.requirements[requirement.index()].value.native_children.push(NativeInvocationAlternative{authority,operation:child}); child
        };
        let RequirementTemplate::Operation { family,call }=self.requirement(child)?.template else { return Err(InferenceError::InvalidScheme) }; self.solve_operation(child,family,call)?;
        let Some(selected)=self.requirement(child)?.candidate.as_ref() else { self.trail_requirement(requirement)?; self.requirements[requirement.index()].value.eligibility=false; return Ok(()) };
        let signature=selected.signature; let Some(binding)=selected.binding.as_ref() else { return Err(InferenceError::InvalidScheme) }; self.work_many(binding.supplied_slots.len()+binding.default_slots.len())?; let binding=self.requirement(child)?.candidate.as_ref().unwrap().binding.clone().unwrap();
        let TypeNode::Arrow(arrow)=self.clone_node(self.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme) };
        let complete=self.requirement(child)?.eligibility;
        self.trail_requirement(requirement)?;
        self.requirements[requirement.index()].value.invocation=Some(InvocationEvidence {callable:original.callable,native_alternatives:vec![NativeInvocationAlternative{authority,operation:child}],plan:InvocationPlan::Unique { signature, binding, timing:if arrow.kind==CallableKind::Stream {InvocationDefaultTiming::AtPull}else{InvocationDefaultTiming::AtCall} },result:original.result,effects:original.effects});
        self.requirements[requirement.index()].value.eligibility=complete; Ok(())
    }
    fn solve_callable_all(&mut self, requirement: RequirementId, invocation: InvocationCallId) -> Result<(), InferenceError> {
        self.work_many(self.invocation_call(invocation)?.arguments.len())?;
        let original = self.invocation_call(invocation)?.clone();
        let TypeNode::NativeCallable(callable) = self.clone_node(self.resolved(original.callable)?)? else { return Err(InferenceError::InvalidScheme) };
        let reason = self.requirement(requirement)?.reason;
        self.work_many(callable.alternatives.len() + original.arguments.len())?;
        let kinds: Vec<_> = original.arguments.iter().map(|argument| argument.kind).collect();
        let mut branches = Vec::with_capacity(callable.alternatives.len()); let mut natives = Vec::new(); let mut complete = true;
        for authority in callable.alternatives {
            self.work()?;
            let (signature, binding) = match authority {
                CallableAuthority::User { signature, .. } => {
                    let TypeNode::Arrow(arrow) = self.clone_node(self.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme) };
                    if !self.invocation_branch_kind_allowed(invocation,arrow.kind)? { return Err(InferenceError::InvalidInvocation { call: invocation, problem: InvocationProblem::CallableKind }); }
                    let binding = self.plan_invocation_arguments(signature, &kinds).map_err(|error| match error { InvocationPlanError::Graph(error) => error, InvocationPlanError::Binding(problem) => InferenceError::InvalidInvocation { call: invocation, problem } })?;
                    if !self.admit_invocation_binding(&arrow,&original.arguments,&binding,invocation,reason,false)? { complete=false; continue; }
                    self.unify(original.result, arrow.result, reason)?;
                    (signature, binding)
                },
                CallableAuthority::Native { authority } => {
                    let child = if let Some(child) = self.native_invocation_children(requirement)?.iter().find(|child| child.authority == authority) { child.operation } else {
                        let family = match authority { NativeAuthority::Single(id) => self.native_contract(id)?.family, NativeAuthority::Family(id) => self.native_family_contract(id)?.family };
                        let level = match self.resolved_effect_summary(original.effects)? { EffectSummary::Variable(id) => slot(&self.effects,id.index(),id.generation)?.level, _ => 0 };
                        let effects = EffectSummary::Variable(self.fresh_derived_effect_at(level,None)?);
                        let child = self.require_operation(family,OperationCall { binding:OperationBinding::Invocation(invocation), effect_mode:OperationEffectMode::ComputedCreation, mono_authority:Some(authority), declared_error_bound:None, receiver:None, arguments:Vec::new(), result:original.result, effects, effect_bindings:Vec::new(), output_effect_bindings:Vec::new() },reason)?;
                        self.trail_requirement(requirement)?; self.requirements[requirement.index()].value.native_children.push(NativeInvocationAlternative { authority, operation:child }); child
                    };
                    let RequirementTemplate::Operation { family,call } = self.requirement(child)?.template else { return Err(InferenceError::InvalidScheme) };
                    self.solve_operation(child,family,call)?; natives.push(NativeInvocationAlternative { authority,operation:child });
                    let Some(selected) = self.requirement(child)?.candidate.as_ref() else { complete = false; continue; };
                    let signature = selected.signature; let Some(binding) = &selected.binding else { return Err(InferenceError::InvalidScheme) };
                    self.work_many(binding.supplied_slots.len() + binding.default_slots.len())?;
                    complete &= self.requirement(child)?.eligibility;
                    (signature,self.requirement(child)?.candidate.as_ref().unwrap().binding.clone().unwrap())
                },
            };
            let TypeNode::Arrow(arrow) = self.clone_node(self.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme) };
            if arrow.kind==CallableKind::Pure {self.include_effects(arrow.effects,EffectSummary::Closed(EffectSet::EMPTY),reason)?;}
            if let Err(error)=self.include_effects(arrow.effects,original.effects,reason) {return Err(self.operation_effect_failure(requirement,arrow.effects,original.effects,error)?);}
            branches.push(InvocationBranchEvidence { authority,signature,binding,timing:if arrow.kind == CallableKind::Stream { InvocationDefaultTiming::AtPull } else { InvocationDefaultTiming::AtCall },effects:arrow.effects });
        }
        if branches.len() == self.node_alternative_count(original.callable)? {
            let mut kinds=0u8;for branch in &branches {self.work()?;let TypeNode::Arrow(arrow)=self.node(self.resolved(branch.signature)?)? else{return Err(InferenceError::InvalidScheme)};kinds|=Self::callable_kind_bit(arrow.kind);}
            if !Self::conditional_kind_allowed(original.domain,kinds) {return Err(InferenceError::InvalidInvocation {call:invocation,problem:InvocationProblem::CallableKind});}
            let mut closed = EffectSet::EMPTY; let mut finite = true; let mut unknown = false;
            for branch in &branches { self.work()?; match self.resolved_effect_summary(branch.effects)? { EffectSummary::Closed(bits) => closed.0 |= bits.0, EffectSummary::Unknown => unknown = true, _ => finite = false } }
            if unknown { if let Err(error)=self.equate_effects(original.effects,EffectSummary::Unknown,reason) {return Err(self.operation_effect_failure(requirement,EffectSummary::Unknown,original.effects,error)?);} } else if finite { if let Err(error)=self.equate_effects(original.effects,EffectSummary::Closed(closed),reason) {return Err(self.operation_effect_failure(requirement,EffectSummary::Closed(closed),original.effects,error)?);} }
            self.trail_requirement(requirement)?;
            self.requirements[requirement.index()].value.invocation = Some(InvocationEvidence { callable:original.callable,native_alternatives:natives,plan:InvocationPlan::All { branches },result:original.result,effects:original.effects });
        }
        self.trail_requirement(requirement)?; self.requirements[requirement.index()].value.eligibility=complete;
        Ok(())
    }
    // A conditional protocol classifies the complete runtime choice. Pure
    // branches remain pure even when another branch makes the aggregate proc.
    pub(super) fn invocation_branch_kind_allowed(&self, invocation:InvocationCallId, kind:CallableKind) -> Result<bool,InferenceError> {
        let call=self.invocation_call(invocation)?;
        let conditional=matches!(self.node(self.resolved(call.callable)?)?,TypeNode::NativeCallable(callable) if callable.alternatives.len()>1);
        Ok(if conditional && call.domain==CallableDomain::Exact(CallableKind::Proc) {matches!(kind,CallableKind::Pure|CallableKind::Proc)} else {call.domain.admits(kind)})
    }
    pub(super) fn callable_kind_bit(kind:CallableKind) -> u8 {match kind {CallableKind::Pure=>1,CallableKind::Proc=>2,CallableKind::Stream=>4}}
    pub(super) fn conditional_kind_allowed(domain:CallableDomain, kinds:u8) -> bool {
        if kinds==0 || kinds&4!=0 && kinds!=4 {return false;}
        match domain {CallableDomain::Pure|CallableDomain::Exact(CallableKind::Pure)=>kinds==1,CallableDomain::Exact(CallableKind::Proc)=>kinds&2!=0&&kinds&4==0,CallableDomain::Exact(CallableKind::Stream)=>kinds==4,CallableDomain::AnyCallable=>true}
    }
    fn node_alternative_count(&self, callable: TypeId) -> Result<usize,InferenceError> { match self.node(self.resolved(callable)?)? { TypeNode::NativeCallable(callable) => Ok(callable.alternatives.len()), _ => Err(InferenceError::InvalidScheme) } }
    fn admit_invocation_binding(&mut self, arrow:&Arrow, arguments:&[InvocationArgument], binding:&InvocationBinding, call_id:InvocationCallId, reason:ReasonId, native_only:bool) -> Result<bool,InferenceError> {
        if let Some(dynamic) = &binding.dynamic {
            for segment in &dynamic.segments {
                match segment {
                    InvocationArgumentSegment::StaticSlot { argument, slot } => self.assign_invocation_argument(&arrow, *slot, arguments[*argument], reason)?,
                    InvocationArgumentSegment::DynamicRange { argument, fixed_slots, rest_slot } => {
                        let argument = arguments[*argument];
                        let actual = if argument.kind == InvocationArgumentKind::PositionalSplice {
                            let ty = self.resolved(argument.ty)?;
                            match self.node(ty)? {
                                TypeNode::List(item) => *item,
                                TypeNode::Meta(meta) => { let level = self.meta(*meta)?.level; let span = self.reason_data(reason)?.span; let item = self.fresh(level, span)?; let list = self.list(item)?; self.unify(ty, list, reason)?; item },
                                TypeNode::Rigid { .. } => return Ok(false),
                                _ => return Err(InferenceError::InvalidInvocation { call: call_id, problem: InvocationProblem::InvalidSplice }),
                            }
                        } else { argument.ty };
                        for slot in fixed_slots { self.work()?; self.assignable(arrow.params[*slot].ty, actual, reason)?; }
                        if let Some(slot) = rest_slot { self.assignable(self.invocation_rest_item(&arrow, *slot)?, actual, reason)?; }
                    }
                }
            }
        } else {
            // Native protocols admit original inputs through their canonical
            // operand relations. Applying ordinary container equality first
            // would erase a declared dynamic leaf before its guard sees it.
            if !native_only { for (argument, slot) in arguments.iter().zip(&binding.supplied_slots) { self.assign_invocation_argument(&arrow, *slot, *argument, reason)?; } }
        }
        Ok(true)
    }
    fn invocation_rest_item(&self, arrow: &Arrow, slot: usize) -> Result<TypeId, InferenceError> {
        match self.node(self.resolved(arrow.params[slot].ty)?)? { TypeNode::List(item) => Ok(*item), _ => Err(InferenceError::InvalidScheme) }
    }
    fn assign_invocation_argument(&mut self, arrow: &Arrow, slot: usize, argument: InvocationArgument, reason: ReasonId) -> Result<(), InferenceError> {
        self.work()?;
        let parameter = &arrow.params[slot];
        let expected = if parameter.rest && argument.kind != InvocationArgumentKind::PositionalSplice { self.invocation_rest_item(arrow, slot)? } else { parameter.ty };
        // Correlated finite inputs are discharged by the retained canonical
        // family requirement, rather than binding an argument to its envelope.
        if matches!(self.node(self.resolved(expected)?)?, TypeNode::FiniteDomain(_)) { return Ok(()); }
        self.assignable(expected, argument.ty, reason)
    }

    /// Planning tracks possible positional cursors, never a guessed splice length.
    /// Named arguments prune impossible cursors in source order; earlier ranges
    /// still retain slots whose conditional collision needs a runtime guard.
    pub fn plan_invocation_arguments(&mut self, signature: TypeId, kinds: &[InvocationArgumentKind]) -> Result<InvocationBinding, InvocationPlanError> {
        let signature = self.resolved(signature)?;
        let TypeNode::Arrow(arrow) = self.clone_node(signature)? else { return Err(InvocationPlanError::Binding(InvocationProblem::NotCallable)) };
        self.work_many(arrow.params.len() + kinds.len())?;
        let mut labels = FxHashMap::default(); let mut rest_slot = None;
        for (slot, parameter) in arrow.params.iter().enumerate() {
            self.work()?;
            if parameter.rest {
                if slot + 1 != arrow.params.len() || parameter.defaulted || !matches!(self.node(self.resolved(parameter.ty)?)?, TypeNode::List(_)) { return Err(InvocationPlanError::Binding(InvocationProblem::InvalidRest)); }
                rest_slot = Some(slot);
            } else { labels.insert(parameter.label, slot); }
        }
        let fixed = rest_slot.unwrap_or(arrow.params.len());
        self.work_many(fixed + 1)?;
        let mut cursors = vec![false; fixed + 1]; cursors[0] = true;
        let mut named = vec![false; fixed]; let mut segments = Vec::with_capacity(kinds.len()); let mut dynamic = false;
        for (argument, kind) in kinds.iter().enumerate() {
            self.work_many((fixed + 1).saturating_mul(3))?;
            let mut next = vec![false; fixed + 1];
            match kind {
                InvocationArgumentKind::Named(label) => {
                    let slot = *labels.get(label).ok_or(InvocationPlanError::Binding(InvocationProblem::UnknownLabel(*label)))?;
                    if named[slot] { return Err(InvocationPlanError::Binding(InvocationProblem::DuplicateArgument(*label))); }
                    for cursor in 0..=slot { next[cursor] = cursors[cursor]; }
                    if !next.iter().any(|live| *live) { return Err(InvocationPlanError::Binding(InvocationProblem::DuplicateArgument(*label))); }
                    named[slot] = true; segments.push(InvocationArgumentSegment::StaticSlot { argument, slot });
                }
                InvocationArgumentKind::Positional | InvocationArgumentKind::PositionalSplice => {
                    let mut destinations = Vec::new(); let mut reaches_rest = false;
                    let mut possible = false;
                    for cursor in 0..=fixed {
                        possible |= cursors[cursor];
                        if named.get(cursor) == Some(&true) { continue; }
                        if *kind == InvocationArgumentKind::PositionalSplice {
                            if possible { next[cursor] = true; if cursor < fixed { destinations.push(cursor); } else { reaches_rest = rest_slot.is_some(); } }
                        } else if cursors[cursor] {
                            if cursor < fixed { destinations.push(cursor); next[cursor + 1] = true; }
                            else if rest_slot.is_some() { reaches_rest = true; next[cursor] = true; }
                        }
                    }
                    if !next.iter().any(|live| *live) { return Err(InvocationPlanError::Binding(InvocationProblem::TooManyArguments)); }
                    let range = *kind == InvocationArgumentKind::PositionalSplice && !destinations.is_empty() || destinations.len() + usize::from(reaches_rest) != 1;
                    if range { dynamic = true; segments.push(InvocationArgumentSegment::DynamicRange { argument, fixed_slots: destinations, rest_slot: if reaches_rest { rest_slot } else { None } }); }
                    else { segments.push(InvocationArgumentSegment::StaticSlot { argument, slot: destinations.first().copied().or(rest_slot).ok_or(InvocationPlanError::Binding(InvocationProblem::TooManyArguments))? }); }
                }
            }
            // A cursor denotes the next positional destination. One forward
            // pass skips any run of names already supplied in source order.
            for cursor in 0..fixed { if named[cursor] && next[cursor] { next[cursor] = false; next[cursor + 1] = true; } }
            cursors = next;
        }
        self.work_many(fixed + 1)?;
        let minimum = cursors.iter().position(|live| *live).ok_or(InvocationPlanError::Binding(InvocationProblem::TooManyArguments))?;
        let maximum = cursors.iter().rposition(|live| *live).unwrap();
        let mut default_slots = Vec::new(); let mut conditional_default_slots = Vec::new(); let mut required_slots = Vec::new();
        for (slot, parameter) in arrow.params[..fixed].iter().enumerate() {
            self.work()?;
            let definitely = named[slot] || slot < minimum; let possibly = named[slot] || slot < maximum;
            if !parameter.defaulted {
                if !possibly { return Err(InvocationPlanError::Binding(InvocationProblem::MissingArgument(parameter.label))); }
                if dynamic { required_slots.push(slot); }
            } else if !definitely { if possibly { conditional_default_slots.push(slot); } else { default_slots.push(slot); } }
        }
        let (supplied_slots, dynamic) = if dynamic {
            (Vec::new(), Some(DynamicInvocationBinding { segments, conditional_default_slots, required_slots, runtime_arity_guard: true, runtime_duplicate_guard: true }))
        } else {
            (segments.into_iter().map(|segment| match segment { InvocationArgumentSegment::StaticSlot { slot, .. } => slot, _ => unreachable!() }).collect(), None)
        };
        Ok(InvocationBinding { supplied_slots, default_slots, rest_slot, dynamic })
    }
}
