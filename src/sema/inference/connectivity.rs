use super::*;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum RelationshipVariable { Type(MetaId), Effect(EffectId) }

pub(super) struct ReachableRequirements {
    pub types: Vec<MetaId>,
    pub effects: Vec<EffectId>,
    pub requirements: Vec<RequirementId>,
}

impl InferenceContext {
    /// Requirements are hyperedges: an obligation incident to one reachable
    /// variable brings its other operands and obligations into the same scheme.
    pub(super) fn requirement_closure(&mut self, types: &[Vec<MetaId>], effects: &[Vec<EffectId>], direct: &[Vec<RequirementId>]) -> Result<Vec<ReachableRequirements>, InferenceError> {
        let mut requirements = Vec::new(); let mut seen = FxHashSet::default();
        for members in direct { for requirement in members { self.work()?; if seen.insert(*requirement) { requirements.push(*requirement); } } }
        let indices: FxHashMap<_, _> = requirements.iter().enumerate().map(|(index, id)| (*id, index)).collect();
        let mut variables = Vec::with_capacity(requirements.len()); let mut incident: FxHashMap<RelationshipVariable, Vec<usize>> = FxHashMap::default();
        for (index, requirement) in requirements.iter().enumerate() {
            let template = self.requirement(*requirement)?.template; let mut keys = FxHashSet::default();
            let residual = self.requirement_is_residual(*requirement)?;
            for ty in if residual { self.requirement_types(template)? } else { Vec::new() } {
                for variable in self.free_metas(ty)? { keys.insert(RelationshipVariable::Type(variable)); }
                for variable in self.free_effects(ty)? { keys.insert(RelationshipVariable::Effect(variable)); }
            }
            if residual { for variable in self.effect_closure(&self.requirement_effects(template)?)? { keys.insert(RelationshipVariable::Effect(variable)); } }
            self.work_many(keys.len())?;
            for key in &keys { incident.entry(*key).or_default().push(index); }
            variables.push(keys);
        }
        let mut closures = Vec::with_capacity(types.len());
        for (member, roots) in types.iter().enumerate() {
            let mut pending: Vec<_> = roots.iter().copied().map(RelationshipVariable::Type).chain(effects[member].iter().copied().map(RelationshipVariable::Effect)).collect();
            let mut reached = FxHashSet::default(); let mut selected = FxHashSet::default();
            while let Some(key) = pending.pop() {
                self.work()?; if !reached.insert(key) { continue; }
                if let Some(edges) = incident.get(&key) {
                    self.work_many(edges.len())?;
                    for index in edges { if selected.insert(*index) { self.work_many(variables[*index].len())?; pending.extend(variables[*index].iter().copied()); } }
                }
            }
            for requirement in &direct[member] {
                self.work()?; let index = *indices.get(requirement).ok_or(InferenceError::InvalidScheme)?;
                if variables[index].is_empty() { selected.insert(index); }
                else if !selected.contains(&index) { return Err(InferenceError::DisconnectedRequirement(*requirement)); }
            }
            let mut selected: Vec<_> = selected.into_iter().collect(); selected.sort_unstable();
            let mut types = Vec::new(); let mut effects = Vec::new();
            for key in reached { match key { RelationshipVariable::Type(id) => types.push(id), RelationshipVariable::Effect(id) => effects.push(id) } }
            types.sort(); effects.sort();
            closures.push(ReachableRequirements { types, effects, requirements: selected.into_iter().map(|index| requirements[index]).collect() });
        }
        Ok(closures)
    }
}
