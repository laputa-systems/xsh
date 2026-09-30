use super::*;
use super::schemes::requirement_types;

impl InferenceContext {
    pub fn requirement_template(&self, id: RequirementId) -> Result<RequirementTemplate, InferenceError> { Ok(self.requirement(id)?.template) }
    pub fn requirement_reason(&self, id: RequirementId) -> Result<ReasonId, InferenceError> { Ok(self.requirement(id)?.reason) }
    pub fn require_add(&mut self, left: TypeId, right: TypeId, result: TypeId, reason: ReasonId) -> Result<RequirementId, InferenceError> {
        self.reason_data(reason)?; self.value_type(left)?; self.value_type(right)?; self.value_type(result)?;
        self.probe(|graph| {
            graph.constraint()?;
            graph.contribute(ConstraintRelation::Add { left, right, result }, reason)?;
            let id = RequirementId { index: graph.requirements.len() as u32, generation: Self::generation()? };
            graph.requirements.push(Slot { generation: id.generation, value: Requirement { template: RequirementTemplate::Add { left, right, result }, reason, evidence: None, queued: false } });
            graph.watch_requirement(id)?; graph.enqueue(id)?;
            Ok(id)
        })
    }
    fn enqueue(&mut self, id: RequirementId) -> Result<(), InferenceError> {
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
    fn watch_requirement(&mut self, id: RequirementId) -> Result<(), InferenceError> {
        let mut pending: Vec<_> = requirement_types(self.requirement(id)?.template).into_iter().map(|ty| (ty, 0usize)).collect();
        let mut seen = FxHashSet::default();
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
            } else { for child in self.children(ty)? { pending.push((child, depth + 1)); } }
        }
        Ok(())
    }
    pub fn solve(&mut self) -> Result<(), InferenceError> {
        self.probe(|graph| {
            while !graph.queue.is_empty() {
                graph.work()?;
                let id = graph.queue.pop_front().unwrap();
                if graph.transactions > 0 { graph.trail.push(Trail::QueuePop(id)); }
                graph.trail_requirement(id)?; graph.requirements[id.index()].value.queued = false;
                graph.solve_add(id)?;
                graph.watch_requirement(id)?;
            }
            Ok(())
        })
    }
    fn solve_add(&mut self, id: RequirementId) -> Result<(), InferenceError> {
        let RequirementTemplate::Add { left, right, result } = self.requirement(id)?.template;
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
    pub fn fresh_effect_at(&mut self, level: u32, upper: Option<EffectSet>) -> Result<EffectId, InferenceError> {
        if upper.is_some_and(|upper| upper.0 & 128 != 0) { return Err(InferenceError::EffectViolation); }
        self.counters.attempted_variables += 1;
        if self.counters.attempted_variables > self.limits.variables as u64 { return Err(InferenceError::Limit("variables")); }
        let id = EffectId { index: self.effects.len() as u32, generation: Self::generation()? };
        self.effects.push(Slot { generation: id.generation, value: EffectVariable { level, bits: EffectSet::EMPTY, upper, outgoing: Vec::new(), incoming: Vec::new(), binding: None } });
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
    pub fn effect_value(&self, id: EffectId) -> Result<EffectSet, InferenceError> {
        match self.resolved_effect_summary(EffectSummary::Variable(id))? {
            EffectSummary::Variable(id) => Ok(slot(&self.effects, id.index(), id.generation)?.bits),
            EffectSummary::Closed(bits) => Ok(bits),
            _ => Err(InferenceError::Boundary("rigid latent effects have no ground summary")),
        }
    }
    fn clone_effect(&mut self, id: EffectId) -> Result<EffectVariable, InferenceError> {
        let variable = slot(&self.effects, id.index(), id.generation)?;
        self.work_many(variable.outgoing.len() + variable.incoming.len())?;
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
            if let EffectSummary::Rigid { scope, index } = self.resolved_effect_summary(EffectSummary::Variable(id))? {
                if !self.scheme(scope)?.effect_quantifiers[index as usize].lower.contains(bits) { return Err(InferenceError::EffectViolation); }
                continue;
            }
            let current = self.clone_effect(id)?; let next = EffectSet(current.bits.0 | bits.0);
            if let Some(upper) = current.upper { if !upper.contains(next) { return Err(InferenceError::EffectViolation); } }
            if next == current.bits { continue; }
            self.trail_effect(id)?; self.effects[id.index()].value.bits = next;
            for successor in current.outgoing { self.work()?; self.counters.wakeups += 1; pending.push((successor, next)); }
        }
        Ok(())
    }
    fn restrict_effect(&mut self, id: EffectId, upper: EffectSet) -> Result<(), InferenceError> {
        let current = self.clone_effect(id)?;
        if !upper.contains(current.bits) { return Err(InferenceError::EffectViolation); }
        let upper = if let Some(previous) = current.upper {
            let mut bits = 0;
            for bit in [EffectSet::FS, EffectSet::NET, EffectSet::PROCESS, EffectSet::ENV, EffectSet::TIME, EffectSet::ERROR, EffectSet::IO] { if previous.contains(bit) && upper.contains(bit) { bits |= bit.0; } }
            EffectSet(bits)
        } else { upper };
        self.trail_effect(id)?; self.effects[id.index()].value.upper = Some(upper); Ok(())
    }
    pub fn include_effects(&mut self, actual: EffectSummary, expected: EffectSummary, reason: ReasonId) -> Result<(), InferenceError> {
        self.reason_data(reason)?;
        self.probe(|graph| { graph.contribute(ConstraintRelation::EffectInclusion { actual, expected }, reason)?; graph.include_effects_inner(actual, expected) })
    }
    pub(super) fn include_effects_inner(&mut self, actual: EffectSummary, expected: EffectSummary) -> Result<(), InferenceError> {
        let actual = self.resolved_effect_summary(actual)?; let expected = self.resolved_effect_summary(expected)?;
        self.work()?;
        self.counters.attempted_constraints += 1;
        if self.counters.attempted_constraints > self.limits.constraints as u64 { return Err(InferenceError::Limit("constraints")); }
        match (actual, expected) {
            (_, EffectSummary::Unknown) => Ok(()),
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
                self.grow_effect(to, current.bits)
            }
            (EffectSummary::Closed(bits), EffectSummary::Rigid { scope, index }) => if self.scheme(scope)?.effect_quantifiers[index as usize].lower.contains(bits) { Ok(()) } else { Err(InferenceError::EffectViolation) },
            (EffectSummary::Rigid { scope, index }, EffectSummary::Closed(upper)) => if upper.contains(self.scheme(scope)?.effect_quantifiers[index as usize].upper.unwrap_or(EffectSet(127))) { Ok(()) } else { Err(InferenceError::EffectViolation) },
            (EffectSummary::Rigid { .. }, EffectSummary::Variable(id)) => self.bind_effect(id, actual),
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
    fn bind_effect(&mut self, id: EffectId, rigid: EffectSummary) -> Result<(), InferenceError> {
        let EffectSummary::Rigid { scope, index } = rigid else { return Err(InferenceError::InvalidScheme) };
        let variable = self.clone_effect(id)?;
        let scheme = self.scheme(scope)?; let quantifier = scheme.effect_quantifiers.get(index as usize).ok_or(InferenceError::InvalidScheme)?;
        if variable.level < scheme.scope_level { return Err(InferenceError::ScopeEscape); }
        if !quantifier.lower.contains(variable.bits) || variable.upper.is_some_and(|upper| !upper.contains(quantifier.upper.unwrap_or(EffectSet(127)))) { return Err(InferenceError::EffectViolation); }
        self.trail_effect(id)?; self.effects[id.index()].value.binding = Some(rigid);
        for successor in variable.outgoing { self.include_effects_inner(rigid, EffectSummary::Variable(successor))?; }
        for predecessor in variable.incoming { self.include_effects_inner(EffectSummary::Variable(predecessor), rigid)?; }
        Ok(())
    }
    pub(super) fn unify_effects(&mut self, left: EffectSummary, right: EffectSummary) -> Result<(), InferenceError> {
        let left = self.resolved_effect_summary(left)?; let right = self.resolved_effect_summary(right)?;
        match (left, right) {
            (EffectSummary::Unknown, EffectSummary::Unknown) => Ok(()),
            (EffectSummary::Unknown, _) | (_, EffectSummary::Unknown) => Err(InferenceError::EffectViolation),
            _ => { self.include_effects_inner(left, right)?; self.include_effects_inner(right, left) }
        }
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
