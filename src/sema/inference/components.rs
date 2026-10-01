use super::*;

impl InferenceContext {
    /// All placeholders stay monomorphic while the component is solved. Shared
    /// variables acquire one rigid owner only after every member's restrictions
    /// have lowered their captures; each member exposes its reachable binders.
    pub fn generalize_component(&mut self, members: &[ComponentMember], environment_level: u32, enclosing_scope: Option<SchemeId>) -> Result<Vec<SchemeId>, InferenceError> {
        self.generalize_component_with_roots(members, environment_level, enclosing_scope, &vec![GeneralizationRoots::default(); members.len()])
    }
    pub fn generalize_component_with_roots(&mut self, members: &[ComponentMember], environment_level: u32, enclosing_scope: Option<SchemeId>, roots: &[GeneralizationRoots]) -> Result<Vec<SchemeId>, InferenceError> {
        if members.is_empty() || members.len() != roots.len() { return Err(InferenceError::InvalidScheme); }
        self.probe(|graph| {
            graph.constraint()?; graph.solve()?;
            let mut semantic_effects = Vec::new();
            for (member, roots) in members.iter().zip(roots) { semantic_effects.extend_from_slice(&roots.effects); for id in graph.free_effects(member.root)? { semantic_effects.push(EffectSummary::Variable(id)); } for requirement in &member.requirements { semantic_effects.extend(graph.requirement_effect_roots(*requirement)?); } }
            graph.seal_derived_effects(&semantic_effects)?; graph.solve()?;
            if let Some(enclosing) = enclosing_scope { graph.scheme(enclosing)?; }
            let mut variables = Vec::with_capacity(members.len()); let mut effects = Vec::with_capacity(members.len());
            let mut captures = Vec::with_capacity(members.len()); let mut effect_captures = Vec::with_capacity(members.len());
            for (index, member) in members.iter().enumerate() {
                let mut free = graph.free_metas(member.root)?;
                let mut explicit_captures = roots[index].captured_types.clone();
                for capture in &explicit_captures { for variable in graph.free_metas(*capture)? { if graph.meta(variable)?.level > environment_level { return Err(InferenceError::ScopeEscape); } free.push(variable); } }
                free.sort(); free.dedup();
                let mut rigid = graph.rigid_nodes(member.root)?;
                for requirement in &member.requirements {
                    for ty in graph.requirement_types(graph.requirement(*requirement)?.template)? {
                        rigid.extend(graph.rigid_nodes(ty)?);
                    }
                }
                for capture in &explicit_captures { rigid.extend(graph.rigid_nodes(*capture)?); }
                let mut rigid: Vec<_> = rigid.into_iter().collect(); rigid.sort();
                for ty in &rigid {
                    if !enclosing_scope.map(|scope| graph.scope_allows_type(scope, *ty)).transpose()?.unwrap_or(false) { return Err(InferenceError::ScopeEscape); }
                }
                let mut rigid_effects = graph.rigid_effects(member.root)?;
                for summary in &roots[index].effects { if let summary @ EffectSummary::Rigid { .. } = graph.resolved_effect_summary(*summary)? { rigid_effects.push(summary); } }
                for summary in &rigid_effects {
                    if !enclosing_scope.map(|scope| graph.scope_allows_effect(scope, *summary)).transpose()?.unwrap_or(false) { return Err(InferenceError::ScopeEscape); }
                }
                if member.policy == Generalization::Monomorphic {
                    for variable in &free {
                        if graph.meta(*variable)?.level > environment_level { graph.trail_meta(*variable)?; graph.metas[variable.index()].value.level = environment_level; graph.counters.level_lowerings += 1; }
                    }
                    graph.lower_effect_levels(member.root, environment_level)?;
                }
                let mut member_effects = graph.free_effects(member.root)?; member_effects.extend(graph.effect_closure(&roots[index].effects)?); member_effects.sort(); member_effects.dedup();
                explicit_captures.extend(rigid);
                let inherited = graph.rigid_effect_inputs(&member_effects)?;
                for summary in inherited { if !enclosing_scope.map(|scope| graph.scope_allows_effect(scope, summary)).transpose()?.unwrap_or(false) { return Err(InferenceError::ScopeEscape); } rigid_effects.push(summary); }
                variables.push(free); effects.push(member_effects); captures.push(explicit_captures); effect_captures.push(rigid_effects);
            }
            let direct: Vec<_> = members.iter().map(|member| member.requirements.clone()).collect();
            let closures = graph.requirement_closure(&variables, &effects, &direct)?;
            let mut reachable_requirements = Vec::with_capacity(members.len());
            for (index, closure) in closures.into_iter().enumerate() {
                variables[index] = closure.types; effects[index] = closure.effects; reachable_requirements.push(closure.requirements);
                for summary in graph.rigid_effect_inputs(&effects[index])? {
                    if !enclosing_scope.map(|scope| graph.scope_allows_effect(scope, summary)).transpose()?.unwrap_or(false) { return Err(InferenceError::ScopeEscape); }
                    effect_captures[index].push(summary);
                }
                let mut seen = FxHashSet::default(); graph.work_many(effect_captures[index].len())?;
                effect_captures[index].retain(|summary| seen.insert(*summary));
                if members[index].policy == Generalization::Monomorphic {
                    for variable in &variables[index] { if graph.meta(*variable)?.level > environment_level { graph.trail_meta(*variable)?; graph.metas[variable.index()].value.level = environment_level; graph.counters.level_lowerings += 1; } }
                    for id in &effects[index] { if graph.effects[id.index()].value.level > environment_level { graph.trail_effect(*id)?; graph.effects[id.index()].value.level = environment_level; graph.counters.level_lowerings += 1; } }
                }
            }
            for (index, member) in members.iter().enumerate() { graph.validate_effect_environment(&effects[index], environment_level, member.policy)?; }
            let mut generalized = Vec::new(); let mut generalized_effects = Vec::new();
            for (index, member) in members.iter().enumerate() {
                for variable in &variables[index] {
                    if member.policy == Generalization::Allowed && graph.meta(*variable)?.level > environment_level { generalized.push(*variable); }
                    else { captures[index].push(graph.meta(*variable)?.ty); }
                }
                for variable in &effects[index] {
                    if member.policy == Generalization::Allowed && graph.effects[variable.index()].value.level > environment_level { generalized_effects.push(*variable); }
                    else { effect_captures[index].push(EffectSummary::Variable(*variable)); }
                }
            }
            generalized.sort(); generalized.dedup(); generalized_effects.sort(); generalized_effects.dedup();
            if members.len() == 1 {
                let member = &members[0];
                let scheme = graph.generalize_with_roots(member.root, environment_level, member.policy, &reachable_requirements[0], &roots[0], enclosing_scope)?;
                graph.schemes[scheme.index()].value.captures = captures.remove(0);
                graph.schemes[scheme.index()].value.effect_captures = effect_captures.remove(0);
                return Ok(vec![scheme]);
            }
            let count = (generalized.len() + generalized_effects.len()) as u64;
            graph.counters.attempted_variables += count; graph.counters.rigid_variables += count;
            if graph.counters.attempted_variables > graph.limits.variables as u64 { return Err(InferenceError::Limit("variables")); }
            graph.counters.attempted_schemes += (members.len() + 1) as u64;
            let owner = SchemeId { index: graph.schemes.len() as u32, generation: Self::generation()? };
            let quantifiers: Vec<_> = generalized.iter().map(|variable| { let meta = graph.meta(*variable).unwrap(); Quantifier { kind: meta.kind, lacks: meta.lacks.clone() } }).collect();
            let effect_quantifiers: Vec<_> = generalized_effects.iter().map(|variable| { let effect = &graph.effects[variable.index()].value; EffectQuantifier { lower: effect.bits, upper: effect.upper, derived: effect.derived } }).collect();
            let effect_binders: Vec<_> = generalized_effects.iter().enumerate().map(|(index, _)| EffectSummary::Rigid { scope: owner, index: index as u32 }).collect();
            graph.schemes.push(Slot { generation: owner.generation, value: Scheme { effect_roots: Vec::new(), scope_owner: owner, role: SchemeRole::ComponentScope, captures: captures.iter().flatten().copied().collect(), effect_captures: effect_captures.iter().flatten().copied().collect(), effect_binders: effect_binders.clone(), body: members[0].root, quantifiers, binders: Vec::new(), effect_quantifiers, effect_inclusions: Vec::new(), requirement_origins: Vec::new(), requirements: Vec::new(), scope_level: environment_level.saturating_add(1) } });
            let mut replacements = FxHashMap::default(); let mut binders = Vec::with_capacity(generalized.len());
            for (index, variable) in generalized.iter().enumerate() {
                let meta = graph.meta(*variable)?; let original = meta.ty; let kind = meta.kind;
                let rigid = graph.allocate(TypeNode::Rigid { scope: owner, index: index as u32, kind })?;
                replacements.insert(original, rigid); binders.push(rigid);
            }
            graph.schemes[owner.index()].value.binders = binders;
            let effect_replacements: FxHashMap<_, _> = generalized_effects.iter().copied().map(EffectSummary::Variable).zip(effect_binders).collect();
            let mut owner_inclusions = Vec::new(); let mut seen_owner_inclusions = FxHashSet::default();
            let type_indices: FxHashMap<_, _> = generalized.iter().enumerate().map(|(index, id)| (*id, index)).collect();
            let effect_indices: FxHashMap<_, _> = generalized_effects.iter().enumerate().map(|(index, id)| (*id, index)).collect();
            let mut memo = ReplacementMemo::default(); let mut replaced_templates = FxHashMap::default(); let mut schemes = Vec::with_capacity(members.len());
            for (index, member) in members.iter().enumerate() {
                graph.work_many(variables[index].len() + effects[index].len())?;
                let selected: Vec<_> = variables[index].iter().filter_map(|variable| type_indices.get(variable).copied()).collect();
                let selected_effects: Vec<_> = effects[index].iter().filter_map(|variable| effect_indices.get(variable).copied()).collect();
                let mut templates = Vec::with_capacity(reachable_requirements[index].len()); let mut requirement_origins = Vec::with_capacity(reachable_requirements[index].len()); let mut seen_templates = FxHashSet::default();
                for requirement in &reachable_requirements[index] {
                    if !graph.requirement_is_residual(*requirement)? { continue; }
                    let template = if let Some(template) = replaced_templates.get(requirement) { *template } else { let template = graph.replace_requirement(graph.requirement(*requirement)?.template, &replacements, &effect_replacements, &mut memo)?; replaced_templates.insert(*requirement, template); template };
                    if seen_templates.insert((template, *requirement)) { templates.push(template); requirement_origins.push(*requirement); }
                }
                let mut inclusions = Vec::new(); let mut seen_inclusions = FxHashSet::default();
                for from in &effects[index] {
                    graph.work_many(graph.effects[from.index()].value.rigid_inputs.len())?;
                    for input in graph.effects[from.index()].value.rigid_inputs.clone() {
                        let output = graph.resolved_effect_summary(EffectSummary::Variable(*from))?;
                        let pair = (*effect_replacements.get(&input).unwrap_or(&input), *effect_replacements.get(&output).unwrap_or(&output));
                        if seen_inclusions.insert(pair) { inclusions.push(pair); }
                        if seen_owner_inclusions.insert(pair) { owner_inclusions.push(pair); }
                    }
                    graph.work_many(graph.effects[from.index()].value.outgoing.len())?;
                    for to in graph.effects[from.index()].value.outgoing.clone() {
                        graph.work()?;
                        let from = graph.resolved_effect_summary(EffectSummary::Variable(*from))?; let to = graph.resolved_effect_summary(EffectSummary::Variable(to))?;
                        let pair = (*effect_replacements.get(&from).unwrap_or(&from), *effect_replacements.get(&to).unwrap_or(&to));
                        if seen_inclusions.insert(pair) { inclusions.push(pair); }
                        if seen_owner_inclusions.insert(pair) { owner_inclusions.push(pair); }
                    }
                }
                let body = graph.replace(member.root, &replacements, &effect_replacements, &mut memo, 0)?;
                let effect_roots = roots[index].effects.iter().map(|summary| { let summary = graph.resolved_effect_summary(*summary)?; Ok(*effect_replacements.get(&summary).unwrap_or(&summary)) }).collect::<Result<Vec<_>, InferenceError>>()?;
                let scope = &graph.schemes[owner.index()].value;
                let value = Scheme { effect_roots, scope_owner: owner, role: SchemeRole::Value, captures: captures[index].clone(), effect_captures: effect_captures[index].clone(), effect_binders: selected_effects.iter().map(|index| scope.effect_binders[*index]).collect(), body, quantifiers: selected.iter().map(|index| scope.quantifiers[*index].clone()).collect(), binders: selected.iter().map(|index| scope.binders[*index]).collect(), effect_quantifiers: selected_effects.iter().map(|index| scope.effect_quantifiers[*index]).collect(), effect_inclusions: inclusions, requirement_origins, requirements: templates, scope_level: scope.scope_level };
                let scheme = SchemeId { index: graph.schemes.len() as u32, generation: Self::generation()? };
                graph.schemes.push(Slot { generation: scheme.generation, value }); schemes.push(scheme);
            }
            graph.schemes[owner.index()].value.effect_inclusions = owner_inclusions;
            graph.schemes[owner.index()].value.body = graph.scheme(schemes[0])?.body;
            for variable in generalized {
                let rigid = replacements[&graph.meta(variable)?.ty]; graph.trail_meta(variable)?;
                graph.metas[variable.index()].value.binding = Some(rigid); graph.wake(variable)?;
            }
            for variable in generalized_effects {
                graph.trail_effect(variable)?; graph.effects[variable.index()].value.binding = Some(effect_replacements[&EffectSummary::Variable(variable)]);
            }
            Ok(schemes)
        })
    }
    pub(super) fn rigid_effects(&mut self, root: TypeId) -> Result<Vec<EffectSummary>, InferenceError> {
        let mut pending = vec![(root, 0usize)]; let mut seen = FxHashSet::default(); let mut effects = Vec::new();
        while let Some((ty, depth)) = pending.pop() {
            self.work()?; if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
            let ty = self.resolved(ty)?; if !seen.insert(ty) { continue; }
            for summary in self.type_effect_roots(ty)? { self.work_many(effects.len() + 1)?; let summary = self.resolved_effect_summary(summary)?; if matches!(summary, EffectSummary::Rigid { .. }) && !effects.contains(&summary) { effects.push(summary); } }
            for child in self.children(ty)? { pending.push((child, depth + 1)); }
        }
        Ok(effects)
    }
    pub(super) fn scope_allows_type(&self, scheme: SchemeId, rigid: TypeId) -> Result<bool, InferenceError> {
        let scheme = self.scheme(scheme)?;
        if scheme.binders.contains(&rigid) { return Ok(true); }
        let mut pending: Vec<_> = scheme.captures.iter().copied().map(|ty| (ty, 0usize)).collect(); let mut seen = FxHashSet::default();
        while let Some((ty, depth)) = pending.pop() {
            if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
            let ty = self.resolved(ty)?; if !seen.insert(ty) { continue; } if ty == rigid { return Ok(true); }
            for child in self.children(ty)? { pending.push((child, depth + 1)); }
        }
        Ok(false)
    }
    pub(super) fn scope_allows_effect(&self, scheme: SchemeId, summary: EffectSummary) -> Result<bool, InferenceError> {
        let scheme = self.scheme(scheme)?;
        if scheme.effect_binders.contains(&summary) { return Ok(true); }
        for capture in &scheme.effect_captures { if self.resolved_effect_summary(*capture)? == summary { return Ok(true); } }
        Ok(false)
    }
}
