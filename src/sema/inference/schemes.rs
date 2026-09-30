use super::*;

impl InferenceContext {
    pub fn scheme_type_binders(&self, scheme: SchemeId) -> Result<Vec<TypeId>, InferenceError> { Ok(self.scheme(scheme)?.binders.clone()) }
    pub fn scheme_effect_binders(&self, scheme: SchemeId) -> Result<Vec<EffectSummary>, InferenceError> {
        Ok((0..self.scheme(scheme)?.effect_quantifiers.len()).map(|index| EffectSummary::Rigid { scope: scheme, index: index as u32 }).collect())
    }
    pub(super) fn free_effects(&mut self, root: TypeId) -> Result<Vec<EffectId>, InferenceError> {
        self.collect_effects(root, None)
    }
    fn collect_effects(&mut self, root: TypeId, cutoff: Option<u32>) -> Result<Vec<EffectId>, InferenceError> {
        let mut types = vec![(root, 0usize)]; let mut seen = FxHashSet::default(); let mut pending = Vec::new();
        while let Some((ty, depth)) = types.pop() {
            self.work()?;
            if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
            let ty = self.resolved(ty)?;
            if !seen.insert(ty) { continue; }
            if let TypeNode::Arrow(arrow) = self.node(ty)? {
                match self.resolved_effect_summary(arrow.effects)? {
                    EffectSummary::Variable(id) => pending.push(id),
                    EffectSummary::Rigid { scope, .. } if cutoff.is_some_and(|cutoff| self.scheme(scope).unwrap().scope_level > cutoff) => return Err(InferenceError::ScopeEscape),
                    _ => {}
                }
            }
            for child in self.children(ty)? { types.push((child, depth + 1)); }
        }
        let mut variables = Vec::new(); let mut seen = FxHashSet::default();
        while let Some(id) = pending.pop() {
            self.work()?;
            let summary = self.resolved_effect_summary(EffectSummary::Variable(id))?;
            let EffectSummary::Variable(id) = summary else { continue };
            if !seen.insert(id) { continue; }
            let variable = slot(&self.effects, id.index(), id.generation)?;
            pending.extend_from_slice(&variable.outgoing); pending.extend_from_slice(&variable.incoming); variables.push(id);
        }
        variables.sort(); Ok(variables)
    }
    pub(super) fn lower_effect_levels(&mut self, root: TypeId, level: u32) -> Result<(), InferenceError> {
        for id in self.collect_effects(root, Some(level))? {
            if self.effects[id.index()].value.level > level { self.trail_effect(id)?; self.effects[id.index()].value.level = level; self.counters.level_lowerings += 1; }
        }
        Ok(())
    }
    fn free_metas(&mut self, root: TypeId) -> Result<Vec<MetaId>, InferenceError> {
        let mut pending = vec![(root, 0usize)]; let mut seen = FxHashSet::default(); let mut variables = Vec::new();
        while let Some((ty, depth)) = pending.pop() {
            self.work()?;
            if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
            let ty = self.resolved(ty)?;
            if !seen.insert(ty) { continue; }
            match self.node(ty)? {
                TypeNode::Meta(id) => variables.push(*id),
                TypeNode::Poison => return Err(InferenceError::Recovery(ty)),
                _ => for child in self.children(ty)? { pending.push((child, depth + 1)); },
            }
        }
        variables.sort(); variables.dedup(); Ok(variables)
    }
    pub fn generalize(&mut self, root: TypeId, environment_level: u32, policy: Generalization, requirements: &[RequirementId]) -> Result<SchemeId, InferenceError> {
        self.probe(|graph| {
            graph.constraint()?; graph.counters.attempted_schemes += 1;
            graph.solve()?;
            let variables = graph.free_metas(root)?;
            let effect_variables = graph.free_effects(root)?;
            let connected: FxHashSet<_> = variables.iter().copied().collect();
            let connected_rigids = graph.rigid_nodes(root)?;
            for requirement in requirements {
                let template = graph.requirement(*requirement)?.template;
                for ty in requirement_types(template) {
                    for variable in graph.free_metas(ty)? { if !connected.contains(&variable) { return Err(InferenceError::DisconnectedRequirement(*requirement)); } }
                    for rigid in graph.rigid_nodes(ty)? { if !connected_rigids.contains(&rigid) { return Err(InferenceError::DisconnectedRequirement(*requirement)); } }
                }
            }
            if policy == Generalization::Monomorphic {
                for variable in &variables {
                    if graph.meta(*variable)?.level > environment_level {
                        graph.trail_meta(*variable)?; graph.metas[variable.index()].value.level = environment_level; graph.counters.level_lowerings += 1;
                    }
                }
                graph.lower_effect_levels(root, environment_level)?;
            }
            let generalized: Vec<_> = if policy == Generalization::Allowed { variables.into_iter().filter(|id| graph.meta(*id).unwrap().level > environment_level).collect() } else { Vec::new() };
            let quantifiers = generalized.iter().map(|id| { let meta = graph.meta(*id).unwrap(); Quantifier { kind: meta.kind, lacks: meta.lacks.clone() } }).collect();
            let generalized_effects: Vec<_> = if policy == Generalization::Allowed { effect_variables.iter().copied().filter(|id| graph.effects[id.index()].value.level > environment_level).collect() } else { Vec::new() };
            let rigid_count = (generalized.len() + generalized_effects.len()) as u64;
            graph.counters.attempted_variables += rigid_count; graph.counters.rigid_variables += rigid_count;
            if graph.counters.attempted_variables > graph.limits.variables as u64 { return Err(InferenceError::Limit("variables")); }
            let effect_quantifiers = generalized_effects.iter().map(|id| { let variable = &graph.effects[id.index()].value; EffectQuantifier { lower: variable.bits, upper: variable.upper } }).collect();
            let scheme = SchemeId { index: graph.schemes.len() as u32, generation: Self::generation()? };
            graph.schemes.push(Slot { generation: scheme.generation, value: Scheme { body: root, quantifiers, binders: Vec::new(), effect_quantifiers, effect_inclusions: Vec::new(), requirements: Vec::new(), scope_level: environment_level.saturating_add(1) } });
            let mut replacements = FxHashMap::default();
            let mut binders = Vec::with_capacity(generalized.len());
            for (index, variable) in generalized.iter().enumerate() {
                let meta = graph.meta(*variable)?;
                let original = meta.ty; let kind = meta.kind;
                let rigid = graph.allocate(TypeNode::Rigid { scope: scheme, index: index as u32, kind })?;
                replacements.insert(original, rigid); binders.push(rigid);
            }
            let effect_replacements: FxHashMap<_, _> = generalized_effects.iter().enumerate().map(|(index, id)| (EffectSummary::Variable(*id), EffectSummary::Rigid { scope: scheme, index: index as u32 })).collect();
            let mut inclusions = Vec::new(); let mut seen_inclusions = FxHashSet::default();
            for from in effect_variables {
                graph.work_many(graph.effects[from.index()].value.outgoing.len())?;
                for to in graph.effects[from.index()].value.outgoing.clone() {
                    graph.work()?;
                    let from = graph.resolved_effect_summary(EffectSummary::Variable(from))?; let to = graph.resolved_effect_summary(EffectSummary::Variable(to))?;
                    let pair = (*effect_replacements.get(&from).unwrap_or(&from), *effect_replacements.get(&to).unwrap_or(&to));
                    if seen_inclusions.insert(pair) { inclusions.push(pair); }
                }
            }
            let mut memo = FxHashMap::default();
            let body = graph.replace(root, &replacements, &effect_replacements, &mut memo, 0)?;
            let mut templates = Vec::with_capacity(requirements.len()); let mut seen_templates = FxHashSet::default();
            for requirement in requirements {
                let RequirementTemplate::Add { left, right, result } = graph.requirement(*requirement)?.template;
                let template = RequirementTemplate::Add { left: graph.replace(left, &replacements, &effect_replacements, &mut memo, 0)?, right: graph.replace(right, &replacements, &effect_replacements, &mut memo, 0)?, result: graph.replace(result, &replacements, &effect_replacements, &mut memo, 0)? };
                if seen_templates.insert(template) { templates.push(template); }
            }
            graph.schemes[scheme.index()].value.body = body;
            graph.schemes[scheme.index()].value.binders = binders;
            graph.schemes[scheme.index()].value.effect_inclusions = inclusions;
            graph.schemes[scheme.index()].value.requirements = templates;
            for variable in generalized {
                let ty = graph.meta(variable)?.ty; let rigid = replacements[&ty];
                graph.trail_meta(variable)?;
                graph.metas[variable.index()].value.binding = Some(rigid);
                graph.wake(variable)?;
            }
            for id in generalized_effects { graph.trail_effect(id)?; graph.effects[id.index()].value.binding = Some(effect_replacements[&EffectSummary::Variable(id)]); }
            Ok(scheme)
        })
    }
    fn rigid_nodes(&mut self, root: TypeId) -> Result<FxHashSet<TypeId>, InferenceError> {
        let mut pending = vec![(root, 0usize)]; let mut seen = FxHashSet::default(); let mut rigids = FxHashSet::default();
        while let Some((ty, depth)) = pending.pop() {
            self.work()?;
            if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
            let ty = self.resolved(ty)?;
            if !seen.insert(ty) { continue; }
            if matches!(self.node(ty)?, TypeNode::Rigid { .. }) { rigids.insert(ty); }
            else { for child in self.children(ty)? { pending.push((child, depth + 1)); } }
        }
        Ok(rigids)
    }
    pub fn instantiate(&mut self, scheme: SchemeId, level: u32, reason: ReasonId) -> Result<Instantiation, InferenceError> {
        let origin = self.reason_data(reason)?.span;
        self.probe(|graph| {
            graph.counters.instantiations += 1;
            let descriptor = graph.scheme(scheme)?;
            let count = descriptor.quantifiers.len() + descriptor.binders.len() + descriptor.requirements.len() + descriptor.effect_quantifiers.len() + descriptor.effect_inclusions.len() + descriptor.quantifiers.iter().map(|quantifier| quantifier.lacks.len()).sum::<usize>();
            graph.work_many(count)?;
            let descriptor = graph.scheme(scheme)?.clone();
            let mut substitutions = Vec::with_capacity(descriptor.quantifiers.len());
            let mut effect_substitutions = Vec::with_capacity(descriptor.effect_quantifiers.len()); let mut effect_replacements = FxHashMap::default();
            for (index, quantifier) in descriptor.effect_quantifiers.iter().enumerate() {
                let id = graph.fresh_effect_at(level, quantifier.upper)?; graph.grow_effect(id, quantifier.lower)?;
                effect_substitutions.push(id); effect_replacements.insert(EffectSummary::Rigid { scope: scheme, index: index as u32 }, EffectSummary::Variable(id));
            }
            for (actual, expected) in &descriptor.effect_inclusions {
                graph.include_effects(*effect_replacements.get(actual).unwrap_or(actual), *effect_replacements.get(expected).unwrap_or(expected), reason)?;
            }
            for quantifier in &descriptor.quantifiers {
                let ty = graph.fresh_kind(quantifier.kind, level, origin)?;
                if let TypeNode::Meta(id) = graph.clone_node(ty)? { graph.metas[id.index()].value.lacks = quantifier.lacks.clone(); }
                substitutions.push(ty);
            }
            let mut replacements = FxHashMap::default();
            let mut pending = vec![(descriptor.body, 0usize)];
            for template in &descriptor.requirements { pending.extend(requirement_types(*template).map(|ty| (ty, 0))); }
            let mut seen = FxHashSet::default();
            while let Some((ty, depth)) = pending.pop() {
                graph.work()?;
                if depth > graph.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
                let ty = graph.resolved(ty)?; if !seen.insert(ty) { continue; }
                if let TypeNode::Rigid { scope, index, kind } = graph.clone_node(ty)? {
                    if scope == scheme {
                        let quantifier = descriptor.quantifiers.get(index as usize).ok_or(InferenceError::InvalidScheme)?;
                        if quantifier.kind != kind { return Err(InferenceError::InvalidScheme); }
                        replacements.insert(ty, substitutions[index as usize]);
                    }
                } else { for child in graph.children(ty)? { pending.push((child, depth + 1)); } }
            }
            let mut memo = FxHashMap::default();
            let ty = graph.replace(descriptor.body, &replacements, &effect_replacements, &mut memo, 0)?;
            let mut requirements = Vec::with_capacity(descriptor.requirements.len());
            for RequirementTemplate::Add { left, right, result } in descriptor.requirements {
                let left = graph.replace(left, &replacements, &effect_replacements, &mut memo, 0)?;
                let right = graph.replace(right, &replacements, &effect_replacements, &mut memo, 0)?;
                let result = graph.replace(result, &replacements, &effect_replacements, &mut memo, 0)?;
                requirements.push(graph.require_add(left, right, result, reason)?);
            }
            Ok(Instantiation { ty, requirements, substitutions, effect_substitutions })
        })
    }
    fn replace(&mut self, ty: TypeId, replacements: &FxHashMap<TypeId, TypeId>, effect_replacements: &FxHashMap<EffectSummary, EffectSummary>, memo: &mut FxHashMap<TypeId, TypeId>, depth: usize) -> Result<TypeId, InferenceError> {
        self.work()?;
        if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
        let ty = self.resolved(ty)?;
        if let Some(replacement) = replacements.get(&ty) { return Ok(*replacement); }
        if let Some(replacement) = memo.get(&ty) { return Ok(*replacement); }
        let original = self.clone_node(ty)?; let record = matches!(original, TypeNode::Record(_));
        let node = match original {
            TypeNode::List(item) => TypeNode::List(self.replace(item, replacements, effect_replacements, memo, depth + 1)?),
            TypeNode::Optional(item) => TypeNode::Optional(self.replace(item, replacements, effect_replacements, memo, depth + 1)?),
            TypeNode::Stream(item) => TypeNode::Stream(self.replace(item, replacements, effect_replacements, memo, depth + 1)?),
            TypeNode::Map(a, b) => TypeNode::Map(self.replace(a, replacements, effect_replacements, memo, depth + 1)?, self.replace(b, replacements, effect_replacements, memo, depth + 1)?),
            TypeNode::Result(a, b) => TypeNode::Result(self.replace(a, replacements, effect_replacements, memo, depth + 1)?, self.replace(b, replacements, effect_replacements, memo, depth + 1)?),
            TypeNode::Arrow(mut arrow) => { let effects = self.resolved_effect_summary(arrow.effects)?; arrow.effects = *effect_replacements.get(&effects).unwrap_or(&effects); for parameter in &mut arrow.params { parameter.ty = self.replace(parameter.ty, replacements, effect_replacements, memo, depth + 1)?; } arrow.result = self.replace(arrow.result, replacements, effect_replacements, memo, depth + 1)?; TypeNode::Arrow(arrow) }
            TypeNode::Module(mut fields) => { for field in &mut fields { field.ty = self.replace(field.ty, replacements, effect_replacements, memo, depth + 1)?; } TypeNode::Module(fields) }
            TypeNode::Record(row) | TypeNode::Row(row) => {
                let descriptor = self.clone_row(row)?; let mut fields = Vec::with_capacity(descriptor.fields.len());
                for field in &descriptor.fields { fields.push(RowField { label: field.label, ty: self.replace(field.ty, replacements, effect_replacements, memo, depth + 1)? }); }
                let tail = descriptor.tail.map(|tail| self.replace(tail, replacements, effect_replacements, memo, depth + 1)).transpose()?;
                let new_row = if fields == descriptor.fields && tail == descriptor.tail { row } else { self.row(fields, tail)? };
                if record { TypeNode::Record(new_row) } else { TypeNode::Row(new_row) }
            }
            other => other,
        };
        let replacement = if &node == self.node(ty)? { ty } else { self.allocate(node)? };
        memo.insert(ty, replacement); Ok(replacement)
    }
}

pub(super) fn requirement_types(template: RequirementTemplate) -> [TypeId; 3] {
    match template { RequirementTemplate::Add { left, right, result } => [left, right, result] }
}
