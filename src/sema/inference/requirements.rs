use super::*;

impl InferenceContext {
    pub fn requirement_origin(&self, id: RequirementId) -> Result<RequirementId, InferenceError> { Ok(self.requirement(id)?.origin) }
    pub fn requirement_source(&self, id: RequirementId) -> Result<RequirementId, InferenceError> { Ok(self.requirement(id)?.source) }
    pub fn requirement_correspondences(&mut self, roots: &[RequirementId]) -> Result<Vec<(RequirementId, RequirementId)>, InferenceError> {
        self.work_many(roots.len())?; let mut pending = roots.to_vec(); let mut seen = FxHashSet::default(); let mut pairs = Vec::new();
        while let Some(id) = pending.pop() {
            self.work()?; if !seen.insert(id) { continue; }
            let source = self.requirement_source(id)?; self.requirement(source)?; pairs.push((source, id));
            self.work_many(self.requirement(id)?.native_children.len())?;
            pending.extend(self.requirement(id)?.native_children.iter().map(|alternative|alternative.operation));
            if let Some(invocation) = &self.requirement(id)?.invocation {
                self.work_many(invocation.native_alternatives.len())?;
                pending.extend(self.requirement(id)?.invocation.as_ref().unwrap().native_alternatives.iter().map(|alternative| alternative.operation));
            }
            if let Some(candidate) = &self.requirement(id)?.candidate {
                self.work_many(candidate.dependencies.len())?;
                pending.extend_from_slice(&self.requirement(id)?.candidate.as_ref().unwrap().dependencies);
            }
        }
        self.work_many(pairs.len().saturating_mul((usize::BITS - pairs.len().leading_zeros()) as usize))?;
        pairs.sort_by_key(|(_, id)| *id); Ok(pairs)
    }
    pub fn include_effects_masked(&mut self, actual: EffectSummary, expected: EffectSummary, excluded: EffectSet, reason: ReasonId) -> Result<RequirementId, InferenceError> {
        self.probe(|graph| {
            graph.resolved_effect_summary(actual)?; graph.resolved_effect_summary(expected)?;
            if excluded.0 & 128 != 0 { return Err(InferenceError::EffectViolation); }
            graph.contribute(ConstraintRelation::MaskedEffectInclusion { actual, expected, excluded }, reason)?;
            graph.new_requirement(RequirementTemplate::EffectInclusion { actual, expected, excluded }, reason)
        })
    }
    pub(super) fn solve_masked_effect_inclusion(&mut self, id: RequirementId, actual: EffectSummary, expected: EffectSummary, excluded: EffectSet) -> Result<(), InferenceError> {
        self.work()?;
        let actual = self.resolved_effect_summary(actual)?; let expected = self.resolved_effect_summary(expected)?;
        if self.masked_effect_tautology(actual, expected, excluded)? { self.trail_requirement(id)?; self.requirements[id.index()].value.eligibility = true; return Ok(()); }
        let masked = |bits: EffectSet| EffectSet(bits.0 & !excluded.0);
        match (actual, expected) {
            (_, EffectSummary::Unknown) => {},
            (EffectSummary::Unknown, _) => self.include_effects_inner(EffectSummary::Unknown, expected)?,
            (EffectSummary::Closed(bits), _) => self.include_effects_inner(EffectSummary::Closed(masked(bits)), expected)?,
            (EffectSummary::Variable(input), _) => {
                let variable = self.clone_effect(input)?;
                if let EffectSummary::Closed(upper) = expected { self.restrict_effect(input, EffectSet(upper.0 | excluded.0))?; }
                else if let EffectSummary::Variable(output) = expected { if let Some(upper) = self.effects[output.index()].value.upper { self.restrict_effect(input, EffectSet(upper.0 | excluded.0))?; } }
                self.include_effects_inner(EffectSummary::Closed(masked(variable.bits)), expected)?;
            }
            (EffectSummary::Rigid { scope, index }, EffectSummary::Closed(upper)) => {
                if !upper.contains(masked(self.scheme(scope)?.effect_quantifiers[index as usize].upper.unwrap_or(EffectSet(127)))) { return Err(InferenceError::EffectViolation); }
            }
            (EffectSummary::Rigid { scope, index }, EffectSummary::Variable(output)) => {
                let quantifier = self.scheme(scope)?.effect_quantifiers[index as usize];
                if let Some(upper) = self.effects[output.index()].value.upper { if !upper.contains(masked(quantifier.upper.unwrap_or(EffectSet(127)))) { return Err(InferenceError::EffectViolation); } }
                self.grow_effect(output, masked(quantifier.lower))?;
            }
            (EffectSummary::Rigid { scope, index }, EffectSummary::Rigid { scope: target, index: target_index }) => {
                let lower = masked(self.scheme(scope)?.effect_quantifiers[index as usize].lower);
                let upper = self.scheme(target)?.effect_quantifiers[target_index as usize].upper.unwrap_or(EffectSet(127));
                if !upper.contains(lower) { return Err(InferenceError::EffectViolation); }
            },
        }
        let complete = !matches!(actual, EffectSummary::Variable(_) | EffectSummary::Rigid { .. }) && !matches!(self.resolved_effect_summary(expected)?, EffectSummary::Variable(_) | EffectSummary::Rigid { .. });
        self.trail_requirement(id)?; self.requirements[id.index()].value.eligibility = complete;
        Ok(())
    }
    pub(super) fn masked_effect_tautology(&self, actual: EffectSummary, expected: EffectSummary, excluded: EffectSet) -> Result<bool, InferenceError> {
        let actual = self.resolved_effect_summary(actual)?; let expected = self.resolved_effect_summary(expected)?;
        if actual == expected || expected == EffectSummary::Unknown { return Ok(true); }
        let upper = match actual {
            EffectSummary::Closed(bits) => bits,
            EffectSummary::Variable(id) => self.effects[id.index()].value.upper.unwrap_or(EffectSet(127)),
            EffectSummary::Rigid { scope, index } => self.scheme(scope)?.effect_quantifiers[index as usize].upper.unwrap_or(EffectSet(127)),
            EffectSummary::Unknown => return Ok(false),
        };
        Ok(upper.0 & !excluded.0 == 0)
    }
    pub fn requirement_template(&self, id: RequirementId) -> Result<RequirementTemplate, InferenceError> { Ok(self.requirement(id)?.template) }
    pub fn requirement_reason(&self, id: RequirementId) -> Result<ReasonId, InferenceError> { Ok(self.requirement(id)?.reason) }
    pub fn require_add(&mut self, left: TypeId, right: TypeId, result: TypeId, reason: ReasonId) -> Result<RequirementId, InferenceError> {
        self.reason_data(reason)?; self.value_type(left)?; self.value_type(right)?; self.value_type(result)?;
        self.probe(|graph| {
            graph.constraint()?;
            graph.contribute(ConstraintRelation::Add { left, right, result }, reason)?;
            let id = RequirementId { index: graph.requirements.len() as u32, generation: Self::generation()? };
            graph.requirements.push(Slot { generation: id.generation, value: Requirement { source: id, origin: id, template: RequirementTemplate::Add { left, right, result }, reason, evidence: None, candidate: None, invocation: None, native_children: Vec::new(), error_join_assignability: None, error_join_output_assignability: None, eligibility: false, queued: false } });
            graph.watch_requirement(id)?; graph.enqueue(id)?;
            Ok(id)
        })
    }
    pub(super) fn enqueue(&mut self, id: RequirementId) -> Result<(), InferenceError> {
        self.work()?;
        if self.requirement(id)?.queued { return Ok(()); }
        self.trail_requirement(id)?; self.requirements[id.index()].value.queued = true;
        self.queue.push_back(id);
        if self.transactions > 0 { self.trail.push(Trail::QueuePush); }
        self.counters.queue_pushes += 1;
        Ok(())
    }
    pub(super) fn wake(&mut self, variable: MetaId) -> Result<(), InferenceError> {
        self.work_many(self.meta(variable)?.watchers.len())?;
        for id in self.meta(variable)?.watchers.clone() { self.work()?; self.counters.wakeups += 1; self.enqueue(id)?; }
        Ok(())
    }
    pub(super) fn watch_requirement(&mut self, id: RequirementId) -> Result<(), InferenceError> {
        let mut pending: Vec<_> = self.requirement_types(self.requirement(id)?.template)?.into_iter().map(|ty| (ty, 0usize)).collect();
        let mut seen = FxHashSet::default();
        let mut effect_roots = self.requirement_effects(self.requirement(id)?.template)?;
        while let Some((ty, depth)) = pending.pop() {
            self.work()?;
            if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
            let ty = self.resolved(ty)?;
            if !seen.insert(ty) { continue; }
            if let TypeNode::Meta(meta) = self.clone_node(ty)? {
                let length = self.meta(meta)?.watchers.len();
                self.work_many((usize::BITS - length.leading_zeros()) as usize)?;
                if let Err(index) = self.meta(meta)?.watchers.binary_search(&id) {
                    self.work_many(length - index)?;
                    if self.transactions > 0 { self.trail.push(Trail::WatcherInsert(meta, index)); }
                    self.metas[meta.index()].value.watchers.insert(index, id);
                }
            } else {
                effect_roots.extend(self.type_effect_roots(ty)?);
                for child in self.children(ty)? { pending.push((child, depth + 1)); }
            }
        }
        for effect in self.effect_closure(&effect_roots)? {
            let length = self.effects[effect.index()].value.watchers.len(); self.work_many((usize::BITS - length.leading_zeros()) as usize)?;
            if let Err(index) = self.effects[effect.index()].value.watchers.binary_search(&id) {
                self.work_many(length - index)?;
                if self.transactions > 0 { self.trail.push(Trail::EffectWatcherInsert(effect, index)); }
                self.effects[effect.index()].value.watchers.insert(index, id);
            }
        }
        Ok(())
    }
    pub(super) fn wake_effect(&mut self, effect: EffectId) -> Result<(), InferenceError> {
        self.work_many(self.effects[effect.index()].value.watchers.len())?;
        for requirement in self.effects[effect.index()].value.watchers.clone() { self.work()?; self.counters.wakeups += 1; self.enqueue(requirement)?; }
        Ok(())
    }
    pub fn solve(&mut self) -> Result<(), InferenceError> {
        self.probe(|graph| {
            while !graph.queue.is_empty() {
                graph.work()?;
                let id = graph.queue.pop_front().unwrap();
                if graph.transactions > 0 { graph.trail.push(Trail::QueuePop(id)); }
                graph.trail_requirement(id)?; graph.requirements[id.index()].value.queued = false;
                match graph.requirement(id)?.template {
                    RequirementTemplate::ErrorJoin {join} => graph.solve_error_join(id,join)?,
                    RequirementTemplate::EffectInclusion { actual, expected, excluded } => graph.solve_masked_effect_inclusion(id, actual, expected, excluded)?,
                    RequirementTemplate::Add { .. } => graph.solve_add(id)?,
                    RequirementTemplate::Eligibility { predicate, ty } => graph.solve_eligibility(id, predicate, ty)?,
                    RequirementTemplate::EqualityCompatible { left, right } => graph.solve_equality_compatible(id, left, right)?,
                    RequirementTemplate::Operation { family, call } => graph.solve_operation(id, family, call)?,
                    RequirementTemplate::CallableInvocation { call } => graph.solve_callable_invocation(id, call)?,
                }
                graph.watch_requirement(id)?;
            }
            Ok(())
        })
    }
    fn solve_add(&mut self, id: RequirementId) -> Result<(), InferenceError> {
        let RequirementTemplate::Add { left, right, result } = self.requirement(id)?.template else { return Err(InferenceError::InvalidScheme) };
        let left = self.resolved(left)?; let right = self.resolved(right)?;
        let left_node = self.clone_node(left)?; let right_node = self.clone_node(right)?;
        if matches!(left_node, TypeNode::Poison) { return Err(InferenceError::Recovery(left)); }
        if matches!(right_node, TypeNode::Poison) { return Err(InferenceError::Recovery(right)); }
        let result_node = self.node(self.resolved(result)?)?;
        if add_domain(&left_node, false) & add_domain(&right_node, false) & add_domain(result_node, true) == 0 { return Err(InferenceError::UnsupportedOperation(id)); }
        if matches!(left_node, TypeNode::Rigid { .. }) || matches!(right_node, TypeNode::Rigid { .. }) { return Ok(()); }
        let selected = match (&left_node, &right_node) {
            (TypeNode::Meta(_), TypeNode::Meta(_)) => return Ok(()),
            (TypeNode::Atom(Atom::Int | Atom::UInt), _) | (TypeNode::Meta(_), TypeNode::Atom(Atom::Int | Atom::UInt)) => SealedOperation::AddInt,
            (TypeNode::Atom(Atom::Float), _) | (TypeNode::Meta(_), TypeNode::Atom(Atom::Float)) => SealedOperation::AddFloat,
            (TypeNode::Atom(Atom::Str), _) | (TypeNode::Meta(_), TypeNode::Atom(Atom::Str)) => SealedOperation::AddStr,
            (TypeNode::Atom(Atom::Duration), _) | (TypeNode::Meta(_), TypeNode::Atom(Atom::Duration)) => SealedOperation::AddDuration,
            (TypeNode::List(_), _) | (TypeNode::Meta(_), TypeNode::List(_)) => SealedOperation::AddList,
            _ => return Err(InferenceError::UnsupportedOperation(id)),
        };
        let output = match selected {
            SealedOperation::AddInt => {
                let int = self.atom(Atom::Int)?;
                for input in [left, right] {
                    match self.node(self.resolved(input)?)? {
                        TypeNode::Atom(Atom::Int | Atom::UInt) => {}
                        TypeNode::Meta(_) => self.unify_inner(input, int, 0)?,
                        _ => return Err(InferenceError::UnsupportedOperation(id)),
                    }
                }
                int
            }
            SealedOperation::AddFloat | SealedOperation::AddStr | SealedOperation::AddDuration => {
                let atom = match selected { SealedOperation::AddFloat => Atom::Float, SealedOperation::AddStr => Atom::Str, _ => Atom::Duration };
                let value = self.atom(atom)?; self.unify_inner(left, value, 0)?; self.unify_inner(right, value, 0)?; value
            }
            SealedOperation::AddList => { self.unify_inner(left, right, 0)?; self.resolved(left)? }
        };
        self.unify_inner(result, output, 0)?;
        let evidence = OperationEvidence { requirement: id, operation: selected, left: self.resolved(left)?, right: self.resolved(right)?, result: self.resolved(result)? };
        self.trail_requirement(id)?; self.requirements[id.index()].value.evidence = Some(evidence);
        Ok(())
    }
    pub fn discharge(&self, requirement: RequirementId) -> Result<Option<OperationEvidence>, InferenceError> { Ok(self.requirement(requirement)?.evidence) }
    pub fn fresh_effect(&mut self, upper: Option<EffectSet>) -> Result<EffectId, InferenceError> {
        self.fresh_effect_at(1, upper)
    }
    /// Execution budgets are solved within the checked program and cannot become
    /// caller supplied latent-effect parameters during value generalization.
    pub fn fresh_execution_effect(&mut self, upper: Option<EffectSet>) -> Result<EffectId, InferenceError> { self.fresh_derived_effect_at(0, upper) }
    pub fn mark_effect_unknown(&mut self, id: EffectId, reason: ReasonId) -> Result<(), InferenceError> { self.include_effects(EffectSummary::Unknown, EffectSummary::Variable(id), reason) }
    pub fn fresh_derived_effect_at(&mut self, level: u32, upper: Option<EffectSet>) -> Result<EffectId, InferenceError> {
        let id = self.fresh_effect_at(level, upper)?; self.effects[id.index()].value.derived = true; Ok(id)
    }
    pub fn fresh_effect_at(&mut self, level: u32, upper: Option<EffectSet>) -> Result<EffectId, InferenceError> {
        if upper.is_some_and(|upper| upper.0 & 128 != 0) { return Err(InferenceError::EffectViolation); }
        self.counters.attempted_variables += 1;
        if self.counters.attempted_variables > self.limits.variables as u64 { return Err(InferenceError::Limit("variables")); }
        let id = EffectId { index: self.effects.len() as u32, generation: Self::generation()? };
        self.effects.push(Slot { generation: id.generation, value: EffectVariable { rigid_inputs: Vec::new(), derived: false, watchers: Vec::new(), level, bits: EffectSet::EMPTY, upper, outgoing: Vec::new(), incoming: Vec::new(), binding: None } });
        Ok(id)
    }
    pub fn resolved_effect_summary(&self, mut summary: EffectSummary) -> Result<EffectSummary, InferenceError> {
        for _ in 0..=self.limits.structural_depth {
            match summary {
                EffectSummary::Variable(id) => {
                    let variable = slot(&self.effects, id.index(), id.generation)?;
                    if let Some(binding) = variable.binding { summary = binding; } else { return Ok(summary); }
                }
                EffectSummary::Rigid { scope, index } => { self.scheme(scope)?.effect_quantifiers.get(index as usize).ok_or(InferenceError::InvalidScheme)?; return Ok(summary); }
                EffectSummary::Closed(bits) if bits.0 & 128 != 0 => return Err(InferenceError::EffectViolation),
                _ => return Ok(summary),
            }
        }
        Err(InferenceError::Limit("effect substitution depth"))
    }
    fn resolve_effect_with_work(&mut self, mut summary: EffectSummary) -> Result<EffectSummary, InferenceError> {
        let mut aliases = Vec::new();
        for _ in 0..=self.limits.structural_depth {
            self.work()?;
            match summary {
                EffectSummary::Variable(id) => {
                    let variable = slot(&self.effects, id.index(), id.generation)?;
                    if let Some(binding) = variable.binding { aliases.push(id); summary = binding; continue; }
                }
                _ => {},
            }
            let resolved = self.resolved_effect_summary(summary)?;
            for alias in aliases {
                if self.effects[alias.index()].value.binding != Some(resolved) { self.trail_effect(alias)?; self.effects[alias.index()].value.binding = Some(resolved); }
            }
            return Ok(resolved);
        }
        Err(InferenceError::Limit("effect substitution depth"))
    }
    pub fn effect_value(&self, id: EffectId) -> Result<EffectSet, InferenceError> {
        match self.resolved_effect_summary(EffectSummary::Variable(id))? {
            EffectSummary::Variable(id) => Ok(slot(&self.effects, id.index(), id.generation)?.bits),
            EffectSummary::Closed(bits) => Ok(bits),
            _ => Err(InferenceError::Boundary("rigid latent effects have no ground summary")),
        }
    }
    fn clone_effect(&mut self, id: EffectId) -> Result<EffectVariable, InferenceError> {
        let variable = slot(&self.effects, id.index(), id.generation)?;
        self.work_many(variable.outgoing.len() + variable.incoming.len() + variable.watchers.len() + variable.rigid_inputs.len())?;
        Ok(slot(&self.effects, id.index(), id.generation)?.clone())
    }
    pub(super) fn trail_effect(&mut self, id: EffectId) -> Result<(), InferenceError> {
        if self.transactions > 0 { let value = self.clone_effect(id)?; self.trail.push(Trail::Effect(id, value)); }
        Ok(())
    }
    pub(super) fn grow_effect(&mut self, id: EffectId, bits: EffectSet) -> Result<(), InferenceError> {
        let mut pending = vec![(id, bits)];
        while let Some((id, bits)) = pending.pop() {
            self.work()?;
            let id = match self.resolve_effect_with_work(EffectSummary::Variable(id))? { EffectSummary::Variable(id) => id, EffectSummary::Unknown => continue, EffectSummary::Closed(closed) => { if !closed.contains(bits) { return Err(InferenceError::EffectViolation); } continue; }, EffectSummary::Rigid { scope, index } => {
                if !self.scheme(scope)?.effect_quantifiers[index as usize].lower.contains(bits) { return Err(InferenceError::EffectViolation); }
                continue;
            } };
            let current = self.clone_effect(id)?; let next = EffectSet(current.bits.0 | bits.0);
            if let Some(upper) = current.upper { if !upper.contains(next) { return Err(InferenceError::EffectViolation); } }
            if next == current.bits { continue; }
            self.trail_effect(id)?; self.effects[id.index()].value.bits = next; self.wake_effect(id)?;
            for successor in current.outgoing { self.work()?; self.counters.wakeups += 1; pending.push((successor, next)); }
        }
        Ok(())
    }
    fn restrict_effect(&mut self, id: EffectId, upper: EffectSet) -> Result<(), InferenceError> {
        // An inclusion constrains both ends: lower permissions flow forward,
        // while an available upper bound constrains every predecessor.
        let mut pending = vec![(id, upper)];
        while let Some((id, upper)) = pending.pop() {
            self.work()?;
            let id = match self.resolve_effect_with_work(EffectSummary::Variable(id))? {
                EffectSummary::Variable(id) => id,
                EffectSummary::Closed(bits) if upper.contains(bits) => continue,
                EffectSummary::Rigid { scope, index } if upper.contains(self.scheme(scope)?.effect_quantifiers[index as usize].upper.unwrap_or(EffectSet(127))) => continue,
                _ => return Err(InferenceError::EffectViolation),
            };
            let current = self.clone_effect(id)?;
            if !upper.contains(current.bits) { return Err(InferenceError::EffectViolation); }
            for input in &current.rigid_inputs {
                self.work()?; let EffectSummary::Rigid { scope, index } = *input else { return Err(InferenceError::InvalidScheme) };
                if !upper.contains(self.scheme(scope)?.effect_quantifiers[index as usize].upper.unwrap_or(EffectSet(127))) { return Err(InferenceError::EffectViolation); }
            }
            let upper = if let Some(previous) = current.upper {
                let mut bits = 0;
                for bit in [EffectSet::FS, EffectSet::NET, EffectSet::PROCESS, EffectSet::ENV, EffectSet::TIME, EffectSet::ERROR, EffectSet::IO] { if previous.contains(bit) && upper.contains(bit) { bits |= bit.0; } }
                EffectSet(bits)
            } else { upper };
            if current.upper == Some(upper) { continue; }
            self.trail_effect(id)?; self.effects[id.index()].value.upper = Some(upper); self.wake_effect(id)?;
            self.work_many(current.incoming.len())?;
            for predecessor in current.incoming { pending.push((predecessor, upper)); }
        }
        Ok(())
    }
    pub fn include_effects(&mut self, actual: EffectSummary, expected: EffectSummary, reason: ReasonId) -> Result<(), InferenceError> {
        self.reason_data(reason)?;
        self.probe(|graph| { graph.contribute(ConstraintRelation::EffectInclusion { actual, expected }, reason)?; graph.include_effects_inner(actual, expected) })
    }
    pub fn equate_effects(&mut self, left: EffectSummary, right: EffectSummary, reason: ReasonId) -> Result<(), InferenceError> {
        self.reason_data(reason)?; self.constraint()?;
        self.probe(|graph| {
            graph.contribute(ConstraintRelation::EffectInclusion { actual: left, expected: right }, reason)?;
            graph.contribute(ConstraintRelation::EffectInclusion { actual: right, expected: left }, reason)?;
            graph.unify_effects(left, right)
        })
    }
    pub(super) fn include_effects_inner(&mut self, actual: EffectSummary, expected: EffectSummary) -> Result<(), InferenceError> {
        let actual = self.resolve_effect_with_work(actual)?; let expected = self.resolve_effect_with_work(expected)?;
        self.work()?;
        self.counters.attempted_constraints += 1;
        if self.counters.attempted_constraints > self.limits.constraints as u64 { return Err(InferenceError::Limit("constraints")); }
        match (actual, expected) {
            (_, EffectSummary::Unknown) => Ok(()),
            (EffectSummary::Unknown, EffectSummary::Variable(id)) => self.propagate_unknown_effect(id),
            (EffectSummary::Unknown, _) => Err(InferenceError::EffectViolation),
            (EffectSummary::Closed(actual), EffectSummary::Closed(expected)) => if expected.contains(actual) { Ok(()) } else { Err(InferenceError::EffectViolation) },
            (EffectSummary::Closed(bits), EffectSummary::Variable(id)) => self.grow_effect(id, bits),
            (EffectSummary::Variable(id), EffectSummary::Closed(upper)) => self.restrict_effect(id, upper),
            (EffectSummary::Variable(from), EffectSummary::Variable(to)) => {
                let current = self.clone_effect(from)?; slot(&self.effects, to.index(), to.generation)?;
                if from != to && !current.outgoing.contains(&to) {
                    self.trail_effect(from)?; self.trail_effect(to)?;
                    self.effects[from.index()].value.outgoing.push(to); self.effects[to.index()].value.incoming.push(from);
                }
                for input in current.rigid_inputs { self.add_derived_rigid_input(to, input)?; }
                if let Some(upper) = self.effects[to.index()].value.upper { self.restrict_effect(from, upper)?; }
                self.grow_effect(to, current.bits)
            }
            (EffectSummary::Closed(bits), EffectSummary::Rigid { scope, index }) => if self.scheme(scope)?.effect_quantifiers[index as usize].lower.contains(bits) { Ok(()) } else { Err(InferenceError::EffectViolation) },
            (EffectSummary::Rigid { scope, index }, EffectSummary::Closed(upper)) => if upper.contains(self.scheme(scope)?.effect_quantifiers[index as usize].upper.unwrap_or(EffectSet(127))) { Ok(()) } else { Err(InferenceError::EffectViolation) },
            (EffectSummary::Rigid { .. }, EffectSummary::Variable(id)) => if self.effects[id.index()].value.derived { self.add_derived_rigid_input(id, actual) } else { self.bind_effect(id, actual) },
            (EffectSummary::Variable(id), EffectSummary::Rigid { .. }) => self.bind_effect(id, expected),
            (EffectSummary::Rigid { scope: a, index: ai }, EffectSummary::Rigid { scope: b, index: bi }) => {
                if a != b { return Err(InferenceError::ScopeEscape); }
                if ai == bi { return Ok(()); }
                let mut pending = vec![actual]; let mut seen = FxHashSet::default();
                while let Some(current) = pending.pop() {
                    self.work_many(1 + self.scheme(a)?.effect_inclusions.len())?;
                    if !seen.insert(current) { continue; }
                    if current == expected { return Ok(()); }
                    for (from, to) in &self.scheme(a)?.effect_inclusions { if *from == current { pending.push(*to); } }
                }
                Err(InferenceError::EffectViolation)
            }
        }
    }
    pub(super) fn validate_effect_environment(&mut self, variables: &[EffectId], environment_level: u32, policy: Generalization) -> Result<(), InferenceError> {
        if policy != Generalization::Allowed { return Ok(()); }
        let mut pending: Vec<_> = variables.iter().copied().filter(|id| self.effects[id.index()].value.level > environment_level).collect();
        let mut seen = FxHashSet::default();
        while let Some(id) = pending.pop() {
            self.work()?; if !seen.insert(id) { continue; }
            let variable = self.clone_effect(id)?;
            for successor in variable.outgoing {
                self.work()?;
                if let EffectSummary::Variable(successor) = self.resolved_effect_summary(EffectSummary::Variable(successor))? {
                    if self.effects[successor.index()].value.level <= environment_level { return Err(InferenceError::ScopeEscape); }
                    pending.push(successor);
                }
            }
        }
        Ok(())
    }
    /// Calculated summaries become finite only after every incoming source is
    /// finite. Unconstrained latent parameters remain symbolic even at empty.
    pub fn seal_derived_effects(&mut self, roots: &[EffectSummary]) -> Result<(), InferenceError> {
        self.probe(|graph| {
            let variables = graph.effect_closure(roots)?; let mut symbolic = FxHashSet::default(); let mut pending = Vec::new(); let mut guarded_outputs = FxHashSet::default();
            for id in &variables {
                graph.work()?; let guarded = graph.pending_operation_effect(*id)?;
                if guarded { guarded_outputs.insert(*id); }
                let variable = graph.clone_effect(*id)?;
                let fixed = variable.upper == Some(variable.bits);
                let opaque_incoming = !variable.rigid_inputs.is_empty() || variable.incoming.iter().map(|id| graph.resolved_effect_summary(EffectSummary::Variable(*id))).collect::<Result<Vec<_>, _>>()?.iter().any(|summary| matches!(summary, EffectSummary::Rigid { .. }));
                if guarded || (!variable.derived && !fixed) || opaque_incoming { pending.push(*id); }
            }
            while let Some(id) = pending.pop() {
                graph.work()?; if !symbolic.insert(id) { continue; }
                let variable = graph.clone_effect(id)?;
                for successor in variable.outgoing { if let EffectSummary::Variable(id) = graph.resolved_effect_summary(EffectSummary::Variable(successor))? { pending.push(id); } }
            }
            let calculated_bounds = graph.calculated_effect_bounds(&variables, &guarded_outputs)?;
            for id in variables {
                graph.work()?;
                if !matches!(graph.resolved_effect_summary(EffectSummary::Variable(id))?, EffectSummary::Variable(_)) { continue; }
                let variable = graph.clone_effect(id)?;
                let calculated_upper = calculated_bounds[&id];
                if variable.derived && calculated_upper != EffectSet(127) { graph.restrict_effect(id, calculated_upper)?; }
                let calculated_fixed = variable.derived && variable.bits.contains(calculated_upper);
                if calculated_fixed || (!symbolic.contains(&id) && (variable.derived || variable.upper == Some(variable.bits))) {
                    graph.trail_effect(id)?; graph.effects[id.index()].value.binding = Some(EffectSummary::Closed(variable.bits)); graph.wake_effect(id)?;
                }
            }
            Ok(())
        })
    }
    fn calculated_effect_bounds(&mut self, variables: &[EffectId], guarded: &FxHashSet<EffectId>) -> Result<FxHashMap<EffectId, EffectSet>, InferenceError> {
        fn covered(bits: EffectSet) -> EffectSet { EffectSet(if bits.0 & EffectSet::IO.0 != 0 { bits.0 | EffectSet::FS.0 | EffectSet::NET.0 | EffectSet::PROCESS.0 | EffectSet::ENV.0 } else { bits.0 }) }
        let mut bounds = FxHashMap::default(); let mut ceilings = FxHashMap::default(); let mut outgoing = FxHashMap::default(); let mut derived = FxHashSet::default(); let mut pending = VecDeque::new();
        for id in variables {
            self.work()?; let variable = self.clone_effect(*id)?; let ceiling = covered(variable.upper.unwrap_or(EffectSet(127)));
            let mut upper = if variable.derived { covered(variable.bits) } else { ceiling };
            for input in variable.rigid_inputs.iter().copied().chain(variable.incoming.iter().map(|id| EffectSummary::Variable(*id))) {
                self.work()?;
                let input = match self.resolved_effect_summary(input)? {
                    EffectSummary::Rigid { scope, index } => self.scheme(scope)?.effect_quantifiers[index as usize].upper.unwrap_or(EffectSet(127)),
                    EffectSummary::Closed(bits) => bits,
                    EffectSummary::Unknown => EffectSet(127),
                    EffectSummary::Variable(_) => continue,
                };
                upper = EffectSet(upper.0 | covered(input).0);
            }
            if guarded.contains(id) { upper = EffectSet(127); } else if variable.derived { upper = EffectSet(upper.0 & ceiling.0); }
            let mut successors = Vec::new();
            for successor in variable.outgoing { self.work()?; if let EffectSummary::Variable(id) = self.resolved_effect_summary(EffectSummary::Variable(successor))? { successors.push(id); } }
            if variable.derived { derived.insert(*id); }
            bounds.insert(*id, upper); ceilings.insert(*id, ceiling); outgoing.insert(*id, successors); pending.push_back(*id);
        }
        // Each permission can enter a calculated node once. Cycles share one
        // least upper solution instead of repeatedly scanning every edge.
        while let Some(id) = pending.pop_front() {
            self.work()?; let upper = bounds[&id]; self.work_many(outgoing[&id].len())?;
            for successor in outgoing[&id].iter().copied() {
                self.work()?;
                if !derived.contains(&successor) || guarded.contains(&successor) { continue; }
                let previous = bounds[&successor]; let next = EffectSet((previous.0 | upper.0) & ceilings[&successor].0);
                if next != previous { bounds.insert(successor, next); pending.push_back(successor); }
            }
        }
        Ok(bounds)
    }
    fn propagate_unknown_effect(&mut self, id: EffectId) -> Result<(), InferenceError> {
        let mut pending = vec![id]; let mut seen = FxHashSet::default();
        while let Some(id) = pending.pop() {
            self.work()?; if !seen.insert(id) { continue; }
            match self.resolved_effect_summary(EffectSummary::Variable(id))? {
                EffectSummary::Unknown => continue,
                EffectSummary::Variable(id) => {
                    let variable = self.clone_effect(id)?;
                    if variable.upper.is_some() { return Err(InferenceError::EffectViolation); }
                    self.trail_effect(id)?; self.effects[id.index()].value.binding = Some(EffectSummary::Unknown); self.wake_effect(id)?;
                    self.work_many(variable.outgoing.len())?; pending.extend(variable.outgoing);
                }
                _ => return Err(InferenceError::EffectViolation),
            }
        }
        Ok(())
    }
    fn add_derived_rigid_input(&mut self, id: EffectId, input: EffectSummary) -> Result<(), InferenceError> {
        let EffectSummary::Rigid { scope, index } = input else { return Err(InferenceError::InvalidScheme) };
        let quantifier = *self.scheme(scope)?.effect_quantifiers.get(index as usize).ok_or(InferenceError::InvalidScheme)?;
        let mut pending = vec![id]; let mut seen = FxHashSet::default();
        while let Some(id) = pending.pop() {
            self.work()?; if !seen.insert(id) { continue; }
            match self.resolved_effect_summary(EffectSummary::Variable(id))? {
                EffectSummary::Variable(id) => {
                    let variable = self.clone_effect(id)?;
                    if variable.level < self.scheme(scope)?.scope_level { return Err(InferenceError::ScopeEscape); }
                    if variable.upper.is_some_and(|upper| !upper.contains(quantifier.upper.unwrap_or(EffectSet(127)))) { return Err(InferenceError::EffectViolation); }
                    self.work_many((usize::BITS - variable.rigid_inputs.len().leading_zeros()) as usize)?;
                    if let Err(position) = variable.rigid_inputs.binary_search_by_key(&(scope, index), |summary| match summary { EffectSummary::Rigid { scope, index } => (*scope, *index), _ => unreachable!() }) {
                        self.constraint()?;
                        self.work_many(variable.rigid_inputs.len() - position)?;
                        if self.transactions > 0 { self.trail.push(Trail::RigidEffectInputInsert(id, position)); }
                        self.effects[id.index()].value.rigid_inputs.insert(position, input); self.wake_effect(id)?;
                    }
                    self.grow_effect(id, quantifier.lower)?;
                    self.work_many(variable.outgoing.len())?; pending.extend(variable.outgoing);
                }
                expected => self.include_effects_inner(input, expected)?,
            }
        }
        Ok(())
    }
    pub(super) fn rigid_effect_inputs(&mut self, variables: &[EffectId]) -> Result<Vec<EffectSummary>, InferenceError> {
        let mut inputs = Vec::new(); let mut seen = FxHashSet::default();
        for id in variables { self.work_many(self.effects[id.index()].value.rigid_inputs.len())?; for input in &self.effects[id.index()].value.rigid_inputs { if seen.insert(*input) { inputs.push(*input); } } }
        Ok(inputs)
    }
    fn bind_effect(&mut self, id: EffectId, rigid: EffectSummary) -> Result<(), InferenceError> {
        let EffectSummary::Rigid { scope, index } = rigid else { return Err(InferenceError::InvalidScheme) };
        let variable = self.clone_effect(id)?;
        let scheme = self.scheme(scope)?; let quantifier = scheme.effect_quantifiers.get(index as usize).ok_or(InferenceError::InvalidScheme)?;
        if variable.level < scheme.scope_level { return Err(InferenceError::ScopeEscape); }
        if !quantifier.lower.contains(variable.bits) || variable.upper.is_some_and(|upper| !upper.contains(quantifier.upper.unwrap_or(EffectSet(127)))) { return Err(InferenceError::EffectViolation); }
        for input in variable.rigid_inputs { self.include_effects_inner(input, rigid)?; }
        self.trail_effect(id)?; self.effects[id.index()].value.binding = Some(rigid); self.wake_effect(id)?;
        for successor in variable.outgoing { self.include_effects_inner(rigid, EffectSummary::Variable(successor))?; }
        for predecessor in variable.incoming { self.include_effects_inner(EffectSummary::Variable(predecessor), rigid)?; }
        Ok(())
    }
    pub(super) fn unify_effects(&mut self, left: EffectSummary, right: EffectSummary) -> Result<(), InferenceError> {
        let left = self.resolve_effect_with_work(left)?; let right = self.resolve_effect_with_work(right)?;
        match (left, right) {
            (EffectSummary::Unknown, EffectSummary::Unknown) => Ok(()),
            (EffectSummary::Variable(left), EffectSummary::Variable(right)) => self.equate_effect_variables(left, right),
            (EffectSummary::Rigid { .. }, EffectSummary::Variable(id)) => self.bind_effect(id, left),
            (EffectSummary::Variable(id), EffectSummary::Rigid { .. }) => self.bind_effect(id, right),
            (EffectSummary::Closed(bits), EffectSummary::Variable(id)) | (EffectSummary::Variable(id), EffectSummary::Closed(bits)) => self.bind_closed_effect(id, bits),
            (EffectSummary::Unknown, EffectSummary::Variable(id)) | (EffectSummary::Variable(id), EffectSummary::Unknown) => self.propagate_unknown_effect(id),
            (EffectSummary::Unknown, _) | (_, EffectSummary::Unknown) => Err(InferenceError::EffectViolation),
            _ => { self.include_effects_inner(left, right)?; self.include_effects_inner(right, left) }
        }
    }
    /// Exact equality joins identities. Permission inclusion keeps separate
    /// nodes because its incoming edges describe calculated union operands.
    fn equate_effect_variables(&mut self, left: EffectId, right: EffectId) -> Result<(), InferenceError> {
        if left == right { return Ok(()); }
        self.constraint()?;
        let a = self.clone_effect(left)?; let b = self.clone_effect(right)?;
        let mut derived = a.derived || b.derived;
        if a.derived != b.derived {
            let (calculated, latent) = if a.derived { (left, right) } else { (right, left) };
            let source = if a.derived { &a } else { &b };
            // An unfinished calculation supplies no finite proof about an
            // independent latent input, even when its current lower set is empty.
            if source.bits == EffectSet::EMPTY && source.incoming.is_empty() && source.rigid_inputs.is_empty() && source.upper != Some(EffectSet::EMPTY) { derived = false; }
            let mut pending = vec![calculated]; let mut seen = FxHashSet::default();
            while let Some(id) = pending.pop() {
                self.work()?;
                let EffectSummary::Variable(id) = self.resolve_effect_with_work(EffectSummary::Variable(id))? else { continue };
                if id == latent { derived = false; break; }
                if !seen.insert(id) { continue; }
                self.work_many(self.effects[id.index()].value.incoming.len())?;
                pending.extend_from_slice(&self.effects[id.index()].value.incoming);
            }
        }
        let (canonical, alias, mut value, other) = if (a.derived && !b.derived && derived) || (a.derived == b.derived && left < right) || (!derived && !a.derived) { (left, right, a, b) } else { (right, left, b, a) };
        let bits = EffectSet(value.bits.0 | other.bits.0);
        let upper = match (value.upper, other.upper) { (Some(a), Some(b)) => {
            let mut bits = 0;
            for bit in [EffectSet::FS, EffectSet::NET, EffectSet::PROCESS, EffectSet::ENV, EffectSet::TIME, EffectSet::ERROR, EffectSet::IO] { if a.contains(bit) && b.contains(bit) { bits |= bit.0; } }
            Some(EffectSet(bits))
        }, (a, b) => a.or(b) };
        if upper.is_some_and(|upper| !upper.contains(bits)) { return Err(InferenceError::EffectViolation); }
        self.work_many(other.incoming.len() + other.outgoing.len() + other.watchers.len() + other.rigid_inputs.len())?;
        value.incoming.extend(other.incoming); value.outgoing.extend(other.outgoing); value.watchers.extend(other.watchers); value.rigid_inputs.extend(other.rigid_inputs);
        value.level = value.level.min(other.level); value.derived = derived;
        let sorting = value.incoming.len() + value.outgoing.len() + value.watchers.len() + value.rigid_inputs.len();
        self.work_many(sorting.saturating_mul((usize::BITS - sorting.leading_zeros()) as usize))?;
        value.incoming.sort(); value.incoming.dedup(); value.outgoing.sort(); value.outgoing.dedup(); value.watchers.sort(); value.watchers.dedup();
        value.rigid_inputs.sort_by_key(|summary| match summary { EffectSummary::Rigid { scope, index } => (*scope, *index), _ => unreachable!() }); value.rigid_inputs.dedup();
        value.incoming.retain(|id| *id != canonical && *id != alias); value.outgoing.retain(|id| *id != canonical && *id != alias);
        self.trail_effect(canonical)?; self.trail_effect(alias)?;
        self.effects[canonical.index()].value = value; self.effects[alias.index()].value.binding = Some(EffectSummary::Variable(canonical));
        self.effects[alias.index()].value.incoming = Vec::new(); self.effects[alias.index()].value.outgoing = Vec::new();
        self.effects[alias.index()].value.watchers = Vec::new(); self.effects[alias.index()].value.rigid_inputs = Vec::new();
        self.grow_effect(canonical, bits)?;
        if let Some(upper) = upper { self.restrict_effect(canonical, upper)?; }
        self.wake_effect(canonical)?;
        let current = self.clone_effect(canonical)?;
        for predecessor in current.incoming { self.include_effects_inner(EffectSummary::Variable(predecessor), EffectSummary::Variable(canonical))?; }
        for successor in current.outgoing { self.include_effects_inner(EffectSummary::Variable(canonical), EffectSummary::Variable(successor))?; }
        Ok(())
    }
    fn bind_closed_effect(&mut self, id: EffectId, bits: EffectSet) -> Result<(), InferenceError> {
        // Equality fixes the summary itself. Inclusion only constrains its
        // bounds and must keep calculated or latent relationships symbolic.
        self.include_effects_inner(EffectSummary::Closed(bits), EffectSummary::Variable(id))?;
        self.include_effects_inner(EffectSummary::Variable(id), EffectSummary::Closed(bits))?;
        let variable = self.clone_effect(id)?;
        self.trail_effect(id)?; self.effects[id.index()].value.binding = Some(EffectSummary::Closed(bits)); self.wake_effect(id)?;
        for predecessor in variable.incoming { self.include_effects_inner(EffectSummary::Variable(predecessor), EffectSummary::Closed(bits))?; }
        for successor in variable.outgoing { self.include_effects_inner(EffectSummary::Closed(bits), EffectSummary::Variable(successor))?; }
        Ok(())
    }
}

fn add_domain(node: &TypeNode, result: bool) -> u8 {
    match node {
        TypeNode::Meta(_) | TypeNode::Rigid { .. } => 31,
        TypeNode::Atom(Atom::Int) => 1,
        TypeNode::Atom(Atom::UInt) if !result => 1,
        TypeNode::Atom(Atom::Float) => 2,
        TypeNode::Atom(Atom::Str) => 4,
        TypeNode::Atom(Atom::Duration) => 8,
        TypeNode::List(_) => 16,
        _ => 0,
    }
}
