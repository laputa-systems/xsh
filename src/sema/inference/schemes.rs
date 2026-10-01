use super::*;

impl InferenceContext {
    pub fn scheme_type_binders(&self, scheme: SchemeId) -> Result<Vec<TypeId>, InferenceError> { Ok(self.scheme(scheme)?.binders.clone()) }
    pub fn scheme_effect_binders(&self, scheme: SchemeId) -> Result<Vec<EffectSummary>, InferenceError> {
        Ok(self.scheme(scheme)?.effect_binders.clone())
    }
    pub fn canonical_scheme_scope(&self, scheme: SchemeId) -> Result<SchemeId, InferenceError> { Ok(self.scheme(scheme)?.scope_owner) }
    pub fn scheme_binder_index(&self, scheme: SchemeId, rigid: TypeId) -> Result<Option<usize>, InferenceError> {
        let rigid = self.resolved(rigid)?;
        Ok(self.scheme(scheme)?.binders.binary_search(&rigid).ok())
    }
    pub fn scheme_effect_binder_index(&self, scheme: SchemeId, summary: EffectSummary) -> Result<Option<usize>, InferenceError> {
        let summary = self.resolved_effect_summary(summary)?;
        let descriptor = self.scheme(scheme)?;
        let EffectSummary::Rigid { scope, index } = summary else { return Ok(descriptor.effect_binders.iter().position(|binder| *binder == summary)); };
        if scope != descriptor.scope_owner { return Ok(None); }
        Ok(descriptor.effect_binders.binary_search_by_key(&index, |binder| match binder { EffectSummary::Rigid { index, .. } => *index, _ => u32::MAX }).ok())
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
            for summary in self.type_effect_roots(ty)? {
                self.work()?;
                match self.resolved_effect_summary(summary)? {
                    EffectSummary::Variable(id) => pending.push(id),
                    EffectSummary::Rigid { scope, .. } if cutoff.is_some_and(|cutoff| self.scheme(scope).unwrap().scope_level > cutoff) => return Err(InferenceError::ScopeEscape),
                    _ => {}
                }
            }
            for child in self.children(ty)? { types.push((child, depth + 1)); }
        }
        self.effect_closure(&pending.into_iter().map(EffectSummary::Variable).collect::<Vec<_>>())
    }
    pub(super) fn effect_closure(&mut self, roots: &[EffectSummary]) -> Result<Vec<EffectId>, InferenceError> {
        let mut pending = Vec::new();
        for summary in roots { if let EffectSummary::Variable(id) = self.resolved_effect_summary(*summary)? { pending.push(id); } }
        let mut variables = Vec::new(); let mut seen = FxHashSet::default();
        while let Some(id) = pending.pop() {
            self.work()?;
            let EffectSummary::Variable(id) = self.resolved_effect_summary(EffectSummary::Variable(id))? else { continue };
            if !seen.insert(id) { continue; }
            let variable = slot(&self.effects, id.index(), id.generation)?;
            let units = variable.outgoing.len() + variable.incoming.len(); self.work_many(units)?;
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
    pub(super) fn free_metas(&mut self, root: TypeId) -> Result<Vec<MetaId>, InferenceError> {
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
        self.generalize_with_roots(root, environment_level, policy, requirements, &GeneralizationRoots::default(), None)
    }
    pub fn generalize_with_effect_roots(&mut self, root: TypeId, environment_level: u32, policy: Generalization, requirements: &[RequirementId], effects: &[EffectSummary]) -> Result<SchemeId, InferenceError> {
        self.generalize_with_roots(root, environment_level, policy, requirements, &GeneralizationRoots { captured_types: Vec::new(), effects: effects.to_vec() }, None)
    }
    pub fn generalize_with_roots(&mut self, root: TypeId, environment_level: u32, policy: Generalization, requirements: &[RequirementId], roots: &GeneralizationRoots, enclosing_scope: Option<SchemeId>) -> Result<SchemeId, InferenceError> {
        self.probe(|graph| {
            graph.constraint()?; graph.counters.attempted_schemes += 1;
            graph.solve()?;
            let mut semantic_effects = roots.effects.clone();
            for id in graph.free_effects(root)? { semantic_effects.push(EffectSummary::Variable(id)); }
            for requirement in requirements { semantic_effects.extend(graph.requirement_effect_roots(*requirement)?); }
            graph.seal_derived_effects(&semantic_effects)?; graph.solve()?;
            let mut variables = graph.free_metas(root)?;
            let mut captures = roots.captured_types.clone();
            let mut connected_rigids = graph.rigid_nodes(root)?;
            for capture in &roots.captured_types {
                let free = graph.free_metas(*capture)?;
                for variable in &free { if graph.meta(*variable)?.level > environment_level { return Err(InferenceError::ScopeEscape); } }
                variables.extend(free); connected_rigids.extend(graph.rigid_nodes(*capture)?);
            }
            for rigid in &connected_rigids {
                if !enclosing_scope.map(|scope| graph.scope_allows_type(scope, *rigid)).transpose()?.unwrap_or(false) { return Err(InferenceError::ScopeEscape); }
                captures.push(*rigid);
            }
            variables.sort(); variables.dedup();
            let mut effect_roots = roots.effects.clone();
            let mut effect_variables = graph.free_effects(root)?;
            effect_variables.extend(graph.effect_closure(&roots.effects)?); effect_variables.sort(); effect_variables.dedup();
            let mut closure = graph.requirement_closure(&[variables], &[effect_variables], &[requirements.to_vec()])?;
            let closure = closure.remove(0); variables = closure.types; let mut effect_variables = closure.effects;
            for requirement in requirements {
                let template = graph.requirement(*requirement)?.template;
                if graph.requirement_is_residual(*requirement)? { effect_roots.extend(graph.requirement_effects(template)?); }
                for ty in graph.requirement_types(template)? {
                    for rigid in graph.rigid_nodes(ty)? { if !connected_rigids.contains(&rigid) { return Err(InferenceError::DisconnectedRequirement(*requirement)); } }
                    if graph.requirement_is_residual(*requirement)? { effect_variables.extend(graph.free_effects(ty)?); }
                }
            }
            let inherited_effects = graph.rigid_effects(root)?;
            effect_roots.extend(inherited_effects); effect_roots.extend(graph.rigid_effect_inputs(&effect_variables)?);
            for summary in &effect_roots {
                if let summary @ EffectSummary::Rigid { .. } = graph.resolved_effect_summary(*summary)? {
                    if !enclosing_scope.map(|scope| graph.scope_allows_effect(scope, summary)).transpose()?.unwrap_or(false) { return Err(InferenceError::ScopeEscape); }
                }
            }
            effect_variables.extend(graph.effect_closure(&effect_roots)?); effect_variables.sort(); effect_variables.dedup();
            for variable in &variables { if graph.meta(*variable)?.level <= environment_level { captures.push(graph.meta(*variable)?.ty); } }
            if policy == Generalization::Monomorphic {
                for variable in &variables {
                    if graph.meta(*variable)?.level > environment_level {
                        graph.trail_meta(*variable)?; graph.metas[variable.index()].value.level = environment_level; graph.counters.level_lowerings += 1;
                    }
                }
                for id in &effect_variables {
                    if graph.effects[id.index()].value.level > environment_level { graph.trail_effect(*id)?; graph.effects[id.index()].value.level = environment_level; graph.counters.level_lowerings += 1; }
                }
            }
            graph.validate_effect_environment(&effect_variables, environment_level, policy)?;
            let generalized: Vec<_> = if policy == Generalization::Allowed { variables.into_iter().filter(|id| graph.meta(*id).unwrap().level > environment_level).collect() } else { Vec::new() };
            let quantifiers = generalized.iter().map(|id| { let meta = graph.meta(*id).unwrap(); Quantifier { kind: meta.kind, lacks: meta.lacks.clone() } }).collect();
            let generalized_effects: Vec<_> = if policy == Generalization::Allowed { effect_variables.iter().copied().filter(|id| graph.effects[id.index()].value.level > environment_level).collect() } else { Vec::new() };
            let rigid_count = (generalized.len() + generalized_effects.len()) as u64;
            graph.counters.attempted_variables += rigid_count; graph.counters.rigid_variables += rigid_count;
            if graph.counters.attempted_variables > graph.limits.variables as u64 { return Err(InferenceError::Limit("variables")); }
            let effect_quantifiers = generalized_effects.iter().map(|id| { let variable = &graph.effects[id.index()].value; EffectQuantifier { lower: variable.bits, upper: variable.upper, derived: variable.derived } }).collect();
            let scheme = SchemeId { index: graph.schemes.len() as u32, generation: Self::generation()? };
            graph.schemes.push(Slot { generation: scheme.generation, value: Scheme { effect_roots: Vec::new(), scope_owner: scheme, role: SchemeRole::Value, captures, effect_captures: effect_roots.iter().copied().filter(|summary| !matches!(summary, EffectSummary::Variable(id) if generalized_effects.contains(id))).collect(), effect_binders: (0..generalized_effects.len()).map(|index| EffectSummary::Rigid { scope: scheme, index: index as u32 }).collect(), body: root, quantifiers, binders: Vec::new(), effect_quantifiers, effect_inclusions: Vec::new(), requirement_origins: Vec::new(), requirements: Vec::new(), scope_level: environment_level.saturating_add(1) } });
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
                graph.work_many(graph.effects[from.index()].value.rigid_inputs.len())?;
                for input in graph.effects[from.index()].value.rigid_inputs.clone() {
                    let output = graph.resolved_effect_summary(EffectSummary::Variable(from))?;
                    let pair = (*effect_replacements.get(&input).unwrap_or(&input), *effect_replacements.get(&output).unwrap_or(&output));
                    if seen_inclusions.insert(pair) { inclusions.push(pair); }
                }
                graph.work_many(graph.effects[from.index()].value.outgoing.len())?;
                for to in graph.effects[from.index()].value.outgoing.clone() {
                    graph.work()?;
                    let from = graph.resolved_effect_summary(EffectSummary::Variable(from))?; let to = graph.resolved_effect_summary(EffectSummary::Variable(to))?;
                    let pair = (*effect_replacements.get(&from).unwrap_or(&from), *effect_replacements.get(&to).unwrap_or(&to));
                    if seen_inclusions.insert(pair) { inclusions.push(pair); }
                }
            }
            let mut memo = ReplacementMemo::default();
            let body = graph.replace(root, &replacements, &effect_replacements, &mut memo, 0)?;
            let mut templates = Vec::with_capacity(requirements.len()); let mut requirement_origins = Vec::with_capacity(requirements.len()); let mut seen_templates = FxHashSet::default();
            for requirement in requirements {
                    if !graph.requirement_is_residual(*requirement)? { continue; }
                let template = graph.replace_requirement(graph.requirement(*requirement)?.template, &replacements, &effect_replacements, &mut memo)?;
                if seen_templates.insert((template, *requirement)) { templates.push(template); requirement_origins.push(*requirement); }
            }
            let stored_effect_roots = effect_roots.iter().map(|summary| { let summary = graph.resolved_effect_summary(*summary)?; Ok(*effect_replacements.get(&summary).unwrap_or(&summary)) }).collect::<Result<Vec<_>, InferenceError>>()?;
            graph.schemes[scheme.index()].value.effect_roots = stored_effect_roots;
            graph.schemes[scheme.index()].value.body = body;
            graph.schemes[scheme.index()].value.binders = binders;
            graph.schemes[scheme.index()].value.effect_inclusions = inclusions;
            graph.schemes[scheme.index()].value.requirements = templates;
            graph.schemes[scheme.index()].value.requirement_origins = requirement_origins;
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
    pub(super) fn rigid_nodes(&mut self, root: TypeId) -> Result<FxHashSet<TypeId>, InferenceError> {
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
            if graph.scheme(scheme)?.role != SchemeRole::Value { return Err(InferenceError::InvalidScheme); }
            let descriptor = graph.scheme(scheme)?;
            let count = descriptor.quantifiers.len() + descriptor.binders.len() + descriptor.requirements.len() + descriptor.requirement_origins.len() + descriptor.effect_quantifiers.len() + descriptor.effect_inclusions.len() + descriptor.effect_roots.len() + descriptor.effect_captures.len() + descriptor.captures.len() + descriptor.quantifiers.iter().map(|quantifier| quantifier.lacks.len()).sum::<usize>();
            graph.work_many(count)?;
            let descriptor = graph.scheme(scheme)?.clone();
            let mut substitutions = Vec::with_capacity(descriptor.quantifiers.len());
            let mut effect_substitutions = Vec::with_capacity(descriptor.effect_quantifiers.len()); let mut effect_replacements = FxHashMap::default();
            for (binder, quantifier) in descriptor.effect_binders.iter().zip(&descriptor.effect_quantifiers) {
                let id = if quantifier.derived { graph.fresh_derived_effect_at(level, quantifier.upper)? } else { graph.fresh_effect_at(level, quantifier.upper)? }; graph.grow_effect(id, quantifier.lower)?;
                effect_substitutions.push(id); effect_replacements.insert(*binder, EffectSummary::Variable(id));
            }
            for (actual, expected) in &descriptor.effect_inclusions {
                graph.include_effects(*effect_replacements.get(actual).unwrap_or(actual), *effect_replacements.get(expected).unwrap_or(expected), reason)?;
            }
            for quantifier in &descriptor.quantifiers {
                let ty = graph.fresh_kind(quantifier.kind, level, origin)?;
                if let TypeNode::Meta(id) = graph.clone_node(ty)? { graph.metas[id.index()].value.lacks = quantifier.lacks.clone(); }
                substitutions.push(ty);
            }
            if descriptor.binders.len() != substitutions.len() || descriptor.effect_binders.len() != effect_substitutions.len() { return Err(InferenceError::InvalidScheme); }
            let replacements = descriptor.binders.iter().copied().zip(substitutions.iter().copied()).collect();
            let mut memo = ReplacementMemo::default();
            memo.sources.extend(descriptor.requirement_origins.iter().copied());
            let ty = graph.replace(descriptor.body, &replacements, &effect_replacements, &mut memo, 0)?;
            if descriptor.requirements.len() != descriptor.requirement_origins.len() { return Err(InferenceError::InvalidScheme); }
            let mut requirements = Vec::with_capacity(descriptor.requirements.len()); let mut requirement_origins = Vec::with_capacity(descriptor.requirements.len());
            for (template, source) in descriptor.requirements.into_iter().zip(descriptor.requirement_origins) {
                let requirement = if let Some(requirement) = memo.requirements.get(&source) { *requirement } else {
                    let template = graph.replace_requirement(template, &replacements, &effect_replacements, &mut memo)?;
                    let origin = graph.requirement_origin(source)?; graph.requirement(origin)?;
                    let requirement = graph.instantiate_requirement(template, reason)?;
                    graph.requirements[requirement.index()].value.source = source; graph.requirements[requirement.index()].value.origin = origin;
                    memo.requirements.insert(source, requirement);
                    graph.replace_native_invocation_children(source, requirement, &replacements, &effect_replacements, &mut memo, 0)?;
                    requirement
                };
                requirements.push(requirement); requirement_origins.push((source, requirement));
            }
            let effect_roots = descriptor.effect_roots.iter().map(|summary| { let summary = graph.resolved_effect_summary(*summary)?; Ok(*effect_replacements.get(&summary).unwrap_or(&summary)) }).collect::<Result<Vec<_>, InferenceError>>()?;
            Ok(Instantiation { requirement_origins, effect_roots, ty, requirements, substitutions, effect_substitutions })
        })
    }
    pub(super) fn replace(&mut self, ty: TypeId, replacements: &FxHashMap<TypeId, TypeId>, effect_replacements: &FxHashMap<EffectSummary, EffectSummary>, memo: &mut ReplacementMemo, depth: usize) -> Result<TypeId, InferenceError> {
        self.work()?;
        if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
        let ty = self.resolved(ty)?;
        if let Some(replacement) = replacements.get(&ty) { return Ok(*replacement); }
        if let Some(replacement) = memo.types.get(&ty) { return Ok(*replacement); }
        let original = self.clone_node(ty)?; let record = matches!(original, TypeNode::Record(_));
        let node = match original {
            TypeNode::List(item) => TypeNode::List(self.replace(item, replacements, effect_replacements, memo, depth + 1)?),
            TypeNode::Optional(item) => {
                let replaced=self.replace(item,replacements,effect_replacements,memo,depth+1)?;
                let replacement=if replaced==item&&!matches!(self.node(self.resolved(replaced)?)?,TypeNode::Optional(_)){ty}else{self.optional(replaced)?};
                memo.types.insert(ty,replacement);return Ok(replacement);
            },
            TypeNode::Stream(item) => TypeNode::Stream(self.replace(item, replacements, effect_replacements, memo, depth + 1)?),
            TypeNode::Map(a, b) => TypeNode::Map(self.replace(a, replacements, effect_replacements, memo, depth + 1)?, self.replace(b, replacements, effect_replacements, memo, depth + 1)?),
            TypeNode::Result(a, b) => TypeNode::Result(self.replace(a, replacements, effect_replacements, memo, depth + 1)?, self.replace(b, replacements, effect_replacements, memo, depth + 1)?),
            TypeNode::Arrow(mut arrow) => { let effects = self.resolved_effect_summary(arrow.effects)?; arrow.effects = *effect_replacements.get(&effects).unwrap_or(&effects); for parameter in &mut arrow.params { parameter.ty = self.replace(parameter.ty, replacements, effect_replacements, memo, depth + 1)?; } arrow.result = self.replace(arrow.result, replacements, effect_replacements, memo, depth + 1)?; TypeNode::Arrow(arrow) }
            TypeNode::CallableChoice(mut signatures) => { for signature in &mut signatures { *signature = self.replace(*signature, replacements, effect_replacements, memo, depth + 1)?; } TypeNode::CallableChoice(signatures) }
            TypeNode::FiniteDomain(mut alternatives) => { for alternative in &mut alternatives { alternative.ty = self.replace(alternative.ty, replacements, effect_replacements, memo, depth + 1)?; } TypeNode::FiniteDomain(alternatives) }
            TypeNode::NativeCallable(mut callable) => {
                callable.signature = self.replace(callable.signature, replacements, effect_replacements, memo, depth + 1)?;
                for authority in &mut callable.alternatives { *authority = match *authority { CallableAuthority::User { signature, origin } => CallableAuthority::User { signature: self.replace(signature, replacements, effect_replacements, memo, depth + 1)?, origin }, CallableAuthority::Native { authority } => CallableAuthority::Native { authority: self.replace_native_authority(authority, replacements, effect_replacements, memo, depth + 1)? } }; }
                TypeNode::NativeCallable(callable)
            }
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
        memo.types.insert(ty, replacement); Ok(replacement)
    }
}
