use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use rustc_hash::{FxHashMap, FxHashSet};
use super::{CallBinding, DeclarationIdentity, ExpressionIdentity, ProducerEffects, ProducerFlowGraph, ProducerFlowId, ProducerFlowKind, ProducerPath, ProducerPathComponent, ProducerProfile};
use crate::sema::inference::{EffectSummary, EffectSet, GraphOwner, InferenceContext, InferenceError, NativeAuthority, ReasonId, RequirementId, InvocationArgumentKind, InvocationArgumentSegment, RequirementTemplate};
use crate::source::Span;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ProducerEvaluationValue {
    pub profile: ProducerProfile,
    pub callable_targets: BTreeMap<ProducerPath, Vec<ProducerEvaluationCallable>>,
    pub opaque_paths: BTreeSet<ProducerPath>,
    pub symbolic_parameters: BTreeMap<ProducerPath, BTreeSet<ProducerEvaluationParameter>>,
    /// Uncertainty at the value root; nested locations remain in pending_paths.
    pub pending: bool,
    pub pending_paths: BTreeSet<ProducerPath>,
    opaque_exclusions: BTreeMap<ProducerPath, BTreeSet<ProducerPath>>,
    pending_exclusions: BTreeMap<ProducerPath, BTreeSet<ProducerPath>>,
}

impl ProducerEvaluationValue {
    pub fn pending_at(&self, path: &ProducerPath) -> bool {
        (self.pending && self.pending_paths.is_empty()) || uncertainty_at(&self.pending_paths, &self.pending_exclusions, path)
    }

    pub fn opaque_at(&self, path: &ProducerPath) -> bool {
        uncertainty_at(&self.opaque_paths, &self.opaque_exclusions, path)
    }

    fn normalize_pending(&mut self) {
        if self.pending && self.pending_paths.is_empty() { self.pending_paths.insert(ProducerPath::default()); }
    }

    fn mark_pending(&mut self) {
        self.pending_paths.insert(ProducerPath::default());
        self.pending_exclusions.remove(&ProducerPath::default());
        self.pending = true;
    }
}

fn uncertainty_at(paths: &BTreeSet<ProducerPath>, exclusions: &BTreeMap<ProducerPath, BTreeSet<ProducerPath>>, demand: &ProducerPath) -> bool {
    paths.iter().any(|origin| demand.0.starts_with(&origin.0) && !exclusions.get(origin).is_some_and(|excluded| excluded.iter().any(|path| demand.0.starts_with(&path.0))))
}

fn merge_uncertainty(graph: &mut InferenceContext, paths: &mut BTreeSet<ProducerPath>, exclusions: &mut BTreeMap<ProducerPath, BTreeSet<ProducerPath>>, incoming: BTreeSet<ProducerPath>, mut incoming_exclusions: BTreeMap<ProducerPath, BTreeSet<ProducerPath>>) -> Result<(), InferenceError> {
    for origin in incoming {
        let excluded = incoming_exclusions.remove(&origin).unwrap_or_default();
        if paths.insert(origin.clone()) {
            if !excluded.is_empty() { exclusions.insert(origin, excluded); }
        } else if let Some(previous) = exclusions.get_mut(&origin) {
            // A union is certain only where every uncertain alternative excludes
            // the subtree. Nested exclusions denote their intersection.
            let comparisons = previous.len().checked_mul(excluded.len()).ok_or(InferenceError::Limit("producer uncertainty work"))?;
            graph.charge_source_fact_work(comparisons as u64)?;
            let mut common = BTreeSet::new();
            for left in previous.iter() {
                for right in &excluded {
                    if left.0.starts_with(&right.0) { common.insert(left.clone()); }
                    else if right.0.starts_with(&left.0) { common.insert(right.clone()); }
                }
            }
            *previous = common;
        }
    }
    exclusions.retain(|_, excluded| !excluded.is_empty());
    Ok(())
}

fn project_uncertainty(graph: &mut InferenceContext, path: &ProducerPath, paths: BTreeSet<ProducerPath>, mut exclusions: BTreeMap<ProducerPath, BTreeSet<ProducerPath>>) -> Result<(BTreeSet<ProducerPath>, BTreeMap<ProducerPath, BTreeSet<ProducerPath>>), InferenceError> {
    let mut result = BTreeSet::new();
    let mut result_exclusions = BTreeMap::new();
    for origin in paths {
        let excluded = exclusions.remove(&origin).unwrap_or_default();
        if excluded.iter().any(|excluded| path.0.starts_with(&excluded.0)) { continue; }
        let projected = if path.0.starts_with(&origin.0) { ProducerPath::default() }
            else if let Some(suffix) = origin.0.strip_prefix(path.0.as_slice()) { ProducerPath(suffix.to_vec()) }
            else { continue; };
        let excluded = excluded.into_iter().filter_map(|excluded| excluded.0.strip_prefix(path.0.as_slice()).map(|suffix| ProducerPath(suffix.to_vec()))).collect::<BTreeSet<_>>();
        merge_uncertainty(graph, &mut result, &mut result_exclusions, BTreeSet::from([projected.clone()]), BTreeMap::from([(projected, excluded)]))?;
    }
    Ok((result, result_exclusions))
}

fn exclude_uncertainty(path: &ProducerPath, paths: &mut BTreeSet<ProducerPath>, exclusions: &mut BTreeMap<ProducerPath, BTreeSet<ProducerPath>>) {
    paths.retain(|origin| !origin.0.starts_with(&path.0));
    exclusions.retain(|origin, _| paths.contains(origin));
    for origin in paths.iter().filter(|origin| path.0.starts_with(&origin.0)) {
        exclusions.entry(origin.clone()).or_default().insert(path.clone());
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct ProducerEvaluationParameter {
    pub declaration: DeclarationIdentity,
    pub index: u32,
    pub path: ProducerPath,
    pub excluded_paths: BTreeSet<ProducerPath>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProducerEvaluationCallable {
    target: ProducerEvaluationCallableTarget,
    environment: std::sync::Arc<ProducerEvaluationEnvironment>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ProducerEvaluationCallableTarget {
    Declaration { declaration: DeclarationIdentity, origin: Option<crate::sema::inference::TypeId> },
    Native(NativeAuthority),
}

impl PartialEq for ProducerEvaluationCallable {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target && Arc::ptr_eq(&self.environment, &other.environment)
    }
}
impl Eq for ProducerEvaluationCallable {}

#[derive(Debug)]
struct ProducerEvaluationEnvironment {
    owner: GraphOwner,
    declaration: Option<DeclarationIdentity>,
    parameters: BTreeMap<u32, ProducerEvaluationValue>,
    parent: Option<std::sync::Arc<ProducerEvaluationEnvironment>>,
    effects: Vec<(EffectSummary, EffectSummary)>,
    requirements: Vec<(RequirementId, RequirementId)>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ProducerEvaluationDeclaration {
    pub parameters: Vec<ProducerFlowId>,
    pub result: Option<ProducerFlowId>,
    pub defaults: BTreeMap<u32, ProducerFlowId>,
    pub enclosing: Option<DeclarationIdentity>,
}

/// Source generation supplies exact formal bindings and effect substitutions.
/// Equal item types do not identify producer values or callable declarations.
#[derive(Debug, Default)]
pub(crate) struct ProducerEvaluationInputs {
    pub declarations: BTreeMap<DeclarationIdentity, ProducerEvaluationDeclaration>,
    pub parameters: BTreeMap<(DeclarationIdentity, u32), ProducerEvaluationValue>,
    pub call_effect_substitutions: BTreeMap<ExpressionIdentity, Vec<(EffectSummary, EffectSummary)>>,
    pub call_bindings: BTreeMap<ExpressionIdentity, CallBinding>,
    pub call_requirement_origins: BTreeMap<ExpressionIdentity, Vec<(RequirementId, RequirementId)>>,
    pub invocation_requirements: BTreeMap<ExpressionIdentity, RequirementId>,
    pub stage_bindings: BTreeMap<super::StageIdentity, CallBinding>,
    pub stage_effect_substitutions: BTreeMap<super::StageIdentity, Vec<(EffectSummary, EffectSummary)>>,
    pub stage_requirement_origins: BTreeMap<super::StageIdentity, Vec<(RequirementId, RequirementId)>>,
    pub stage_invocation_requirements: BTreeMap<super::StageIdentity, RequirementId>,
    pub stage_invocation_projections: BTreeMap<super::StageIdentity, (RequirementId, usize)>,
    pub origins: rustc_hash::FxHashMap<ProducerFlowId, Span>,
    pub level: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ProducerArgumentTransfer {
    pub argument: usize,
    pub slot: usize,
    pub splice: bool,
    pub rest: bool,
}

/// Producer locations follow the already checked binding, including every
/// possible destination of an unknown-length segment. This does not choose
/// argument cardinality or bind a callable again.
pub(super) fn producer_argument_transfers(graph: &mut InferenceContext, binding: &CallBinding, kinds: &[InvocationArgumentKind]) -> Result<Vec<ProducerArgumentTransfer>, InferenceError> {
    let mut transfers = Vec::new();
    if let Some(dynamic) = &binding.dynamic {
        graph.charge_source_fact_work(dynamic_work(dynamic) as u64 + kinds.len() as u64 + binding.default_slots.len() as u64)?;
        let definite_defaults: FxHashSet<_> = binding.default_slots.iter().copied().collect();
        for segment in &dynamic.segments {
            let (argument, slots, rest) = match segment {
                InvocationArgumentSegment::StaticSlot { argument, slot } => (*argument, std::slice::from_ref(slot), None),
                InvocationArgumentSegment::DynamicRange { argument, fixed_slots, rest_slot } => (*argument, fixed_slots.as_slice(), *rest_slot),
            };
            let kind = *kinds.get(argument).ok_or(InferenceError::Boundary("producer segment has no source argument"))?;
            for slot in slots.iter().copied().chain(rest) {
                // Later named arguments can exclude destinations from an earlier
                // speculative range. A slot with a definite default receives
                // no supplied value in an accepted invocation.
                if matches!(segment, InvocationArgumentSegment::DynamicRange { .. }) && definite_defaults.contains(&slot) { continue; }
                transfers.push(ProducerArgumentTransfer { argument, slot, splice: kind == InvocationArgumentKind::PositionalSplice, rest: Some(slot) == binding.rest_slot });
            }
        }
    } else {
        if binding.supplied_slots.len() != kinds.len() { return Err(InferenceError::Boundary("producer arguments disagree with call binding evidence")); }
        graph.charge_source_fact_work(kinds.len() as u64)?;
        for (argument, (&slot, kind)) in binding.supplied_slots.iter().zip(kinds).enumerate() {
            transfers.push(ProducerArgumentTransfer { argument, slot, splice: *kind == InvocationArgumentKind::PositionalSplice, rest: Some(slot) == binding.rest_slot });
        }
    }
    Ok(transfers)
}

fn dynamic_work(binding: &crate::sema::inference::DynamicInvocationBinding) -> usize {
    binding.segments.len() + binding.conditional_default_slots.len() + binding.required_slots.len()
        + binding.segments.iter().map(|segment| match segment { InvocationArgumentSegment::DynamicRange { fixed_slots, .. } => fixed_slots.len(), _ => 0 }).sum::<usize>()
}

struct EvaluationBinding {
    plan: CallBinding,
    kinds: Vec<InvocationArgumentKind>,
}

pub(crate) fn evaluate_producer_flow(
    graph: &mut InferenceContext,
    flows: &ProducerFlowGraph,
    root: ProducerFlowId,
    inputs: &ProducerEvaluationInputs,
) -> Result<ProducerEvaluationValue, InferenceError> {
    Ok(evaluate_producer_flows(graph, flows, &[root], inputs)?.pop().unwrap())
}

pub(crate) fn evaluate_producer_flow_with_registry(
    graph: &mut InferenceContext,
    flows: &ProducerFlowGraph,
    root: ProducerFlowId,
    inputs: &ProducerEvaluationInputs,
    registry: &crate::sema::registry_graph::RegistryGraph,
) -> Result<ProducerEvaluationValue, InferenceError> {
    Ok(evaluate_producer_flows_with_registry(graph, flows, &[root], inputs, Some(registry))?.pop().unwrap())
}

/// Roots share a single immutable input snapshot and one lexical environment.
/// Each dependency is revisited only when an observed transfer value changes.
pub(crate) fn evaluate_producer_flows(
    graph: &mut InferenceContext,
    flows: &ProducerFlowGraph,
    roots: &[ProducerFlowId],
    inputs: &ProducerEvaluationInputs,
) -> Result<Vec<ProducerEvaluationValue>, InferenceError> {
    evaluate_producer_flows_with_registry(graph, flows, roots, inputs, None)
}

fn evaluate_producer_flows_with_registry(
    graph: &mut InferenceContext,
    flows: &ProducerFlowGraph,
    roots: &[ProducerFlowId],
    inputs: &ProducerEvaluationInputs,
    registry: Option<&crate::sema::registry_graph::RegistryGraph>,
) -> Result<Vec<ProducerEvaluationValue>, InferenceError> {
    if graph.owner() != flows.owner() { return Err(InferenceError::ForeignHandle); }
    for root in roots { flows.node(*root)?; }
    graph.charge_source_fact_work(1)?;
    graph.charge_source_fact_nodes(1)?;
    let environment = Arc::new(ProducerEvaluationEnvironment {
        owner: graph.owner(), declaration: None, parameters: BTreeMap::new(), parent: None, effects: Vec::new(), requirements: Vec::new(),
    });
    let mut evaluator = Evaluator {
        graph, flows, inputs, registry, entries: Vec::new(), entry_ids: FxHashMap::default(),
        environments: FxHashMap::default(), queue: VecDeque::new(), unions: BTreeMap::new(), reasons: FxHashMap::default(), root_environment: environment.clone(),
    };
    let mut entries = Vec::with_capacity(roots.len());
    for root in roots { entries.push(evaluator.demand(*root, environment.clone())?); }
    while let Some(entry) = evaluator.queue.pop_front() {
        evaluator.entries[entry].queued = false;
        evaluator.graph.charge_source_fact_work(1)?;
        let value = evaluator.transfer(entry)?;
        if value != evaluator.entries[entry].value {
            evaluator.entries[entry].value = value;
            let dependents = evaluator.entries[entry].dependents.iter().copied().collect::<Vec<_>>();
            evaluator.graph.charge_source_fact_work(dependents.len() as u64)?;
            for dependent in dependents { evaluator.enqueue(dependent); }
        }
    }
    let mut values = Vec::with_capacity(entries.len());
    for entry in entries {
        evaluator.graph.charge_source_fact_work(value_size(&evaluator.entries[entry].value) as u64)?;
        values.push(evaluator.entries[entry].value.clone());
    }
    Ok(values)
}

#[derive(Debug, Eq, PartialEq)]
struct EnvironmentKey {
    declaration: DeclarationIdentity,
    parent: usize,
    parameters: BTreeMap<u32, ProducerEvaluationValue>,
    effects: Vec<(EffectSummary, EffectSummary)>,
    requirements: Vec<(RequirementId, RequirementId)>,
}

fn hash_declaration<H: Hasher>(declaration: DeclarationIdentity, hash: &mut H) {
    declaration.source.hash(hash);
    declaration.namespace.hash(hash);
    declaration.declaration.hash(hash);
}

fn hash_path<H: Hasher>(path: &ProducerPath, hash: &mut H) {
    path.0.len().hash(hash);
    for part in &path.0 {
        std::mem::discriminant(part).hash(hash);
        match part { ProducerPathComponent::RecordField(name) => name.hash(hash), ProducerPathComponent::CallableParameter(index) => index.hash(hash), _ => {} }
    }
}

fn hash_value<H: Hasher>(value: &ProducerEvaluationValue, hash: &mut H) {
    value.pending.hash(hash);
    value.pending_paths.len().hash(hash);
    for path in &value.pending_paths { hash_path(path, hash); }
    for exclusions in [&value.opaque_exclusions, &value.pending_exclusions] {
        exclusions.len().hash(hash);
        for (origin, paths) in exclusions { hash_path(origin, hash); paths.len().hash(hash); for path in paths { hash_path(path, hash); } }
    }
    value.profile.len().hash(hash);
    for (path, effects) in &value.profile { hash_path(path, hash); effects.pull.hash(hash); effects.close.hash(hash); }
    value.opaque_paths.len().hash(hash);
    for path in &value.opaque_paths { hash_path(path, hash); }
    value.symbolic_parameters.len().hash(hash);
    for (path, parameters) in &value.symbolic_parameters {
        hash_path(path, hash); parameters.len().hash(hash);
        for parameter in parameters { hash_declaration(parameter.declaration, hash); parameter.index.hash(hash); hash_path(&parameter.path, hash); parameter.excluded_paths.len().hash(hash); for path in &parameter.excluded_paths { hash_path(path, hash); } }
    }
    value.callable_targets.len().hash(hash);
    for (path, targets) in &value.callable_targets {
        hash_path(path, hash); targets.len().hash(hash);
        for target in targets {
            std::mem::discriminant(&target.target).hash(hash);
            match target.target {
                ProducerEvaluationCallableTarget::Declaration { declaration, origin } => { hash_declaration(declaration, hash); origin.hash(hash); },
                ProducerEvaluationCallableTarget::Native(contract) => contract.hash(hash),
            }
            (Arc::as_ptr(&target.environment) as usize).hash(hash);
        }
    }
}

impl Hash for EnvironmentKey {
    fn hash<H: Hasher>(&self, hash: &mut H) {
        hash_declaration(self.declaration, hash);
        self.parent.hash(hash);
        self.effects.hash(hash);
        self.requirements.hash(hash);
        self.parameters.len().hash(hash);
        for (index, value) in &self.parameters { index.hash(hash); hash_value(value, hash); }
    }
}

struct EvaluationEntry {
    flow: ProducerFlowId,
    environment: Arc<ProducerEvaluationEnvironment>,
    value: ProducerEvaluationValue,
    dependents: FxHashSet<usize>,
    queued: bool,
}

struct EffectUnion {
    output: EffectSummary,
    inputs: FxHashSet<EffectSummary>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum UnionPurpose { Result, Parameter(DeclarationIdentity, u32) }

#[derive(Clone, Copy)]
enum EvaluationCall { Expression(ExpressionIdentity), Stage(super::StageIdentity) }

struct Evaluator<'a> {
    graph: &'a mut InferenceContext,
    flows: &'a ProducerFlowGraph,
    inputs: &'a ProducerEvaluationInputs,
    registry: Option<&'a crate::sema::registry_graph::RegistryGraph>,
    entries: Vec<EvaluationEntry>,
    entry_ids: FxHashMap<(ProducerFlowId, usize), usize>,
    environments: FxHashMap<EnvironmentKey, Arc<ProducerEvaluationEnvironment>>,
    queue: VecDeque<usize>,
    unions: BTreeMap<(usize, UnionPurpose, ProducerPath, bool), EffectUnion>,
    reasons: FxHashMap<ProducerFlowId, ReasonId>,
    root_environment: Arc<ProducerEvaluationEnvironment>,
}

fn value_size(value: &ProducerEvaluationValue) -> usize {
    1 + value.profile.keys().map(|path| 1 + path.0.len()).sum::<usize>()
        + value.opaque_paths.iter().chain(&value.pending_paths).map(|path| 1 + path.0.len()).sum::<usize>()
        + [&value.opaque_exclusions, &value.pending_exclusions].into_iter().map(|exclusions| exclusions.iter().map(|(origin, paths)| 1 + origin.0.len() + paths.iter().map(|path| 1 + path.0.len()).sum::<usize>()).sum::<usize>()).sum::<usize>()
        + value.callable_targets.iter().map(|(path, targets)| 1 + path.0.len() + targets.len()).sum::<usize>()
        + value.symbolic_parameters.iter().map(|(path, parameters)| 1 + path.0.len() + parameters.iter().map(|parameter| 1 + parameter.path.0.len() + parameter.excluded_paths.iter().map(|path| 1 + path.0.len()).sum::<usize>()).sum::<usize>()).sum::<usize>()
}

impl Evaluator<'_> {
    fn enqueue(&mut self, entry: usize) {
        if !self.entries[entry].queued { self.entries[entry].queued = true; self.queue.push_back(entry); }
    }

    fn demand(&mut self, flow: ProducerFlowId, environment: Arc<ProducerEvaluationEnvironment>) -> Result<usize, InferenceError> {
        if environment.owner != self.graph.owner() { return Err(InferenceError::ForeignHandle); }
        self.flows.node(flow)?;
        self.graph.charge_source_fact_work(1)?;
        let key = (flow, Arc::as_ptr(&environment) as usize);
        if let Some(entry) = self.entry_ids.get(&key) { return Ok(*entry); }
        self.graph.charge_source_fact_nodes(1)?;
        let entry = self.entries.len();
        self.entries.push(EvaluationEntry { flow, environment, value: ProducerEvaluationValue::default(), dependents: FxHashSet::default(), queued: false });
        self.entry_ids.insert(key, entry);
        self.enqueue(entry);
        Ok(entry)
    }

    fn read(&mut self, flow: ProducerFlowId, environment: Arc<ProducerEvaluationEnvironment>, dependent: usize) -> Result<ProducerEvaluationValue, InferenceError> {
        let entry = self.demand(flow, environment)?;
        if !self.entries[entry].dependents.contains(&dependent) {
            self.graph.charge_source_fact_edges(1)?;
            self.entries[entry].dependents.insert(dependent);
        }
        self.graph.charge_source_fact_work(value_size(&self.entries[entry].value) as u64)?;
        Ok(self.entries[entry].value.clone())
    }

    fn substitute_effect(&mut self, mut effect: EffectSummary, environment: &Arc<ProducerEvaluationEnvironment>) -> Result<EffectSummary, InferenceError> {
        let mut visited = FxHashSet::default();
        for _ in 0..=self.graph.limits().structural_depth {
            self.graph.charge_source_fact_work(1)?;
            effect = self.graph.resolved_effect_summary(effect)?;
            if !visited.insert(effect) { return Err(InferenceError::Boundary("cyclic producer effect substitution")); }
            let mut frame = Some(environment);
            let mut replacement = None;
            while let Some(current) = frame {
                self.graph.charge_source_fact_work(current.effects.len() as u64 + 1)?;
                for (formal, actual) in &current.effects {
                    if self.graph.resolved_effect_summary(*formal)? == effect && *actual != effect { replacement = Some(*actual); break; }
                }
                if replacement.is_some() { break; }
                frame = current.parent.as_ref();
            }
            match replacement { Some(actual) => effect = actual, None => return self.graph.resolved_effect_summary(effect) }
        }
        Err(InferenceError::Limit("producer effect substitution depth"))
    }

    fn reason(&mut self, entry: usize) -> Result<ReasonId, InferenceError> {
        let flow = self.entries[entry].flow;
        if let Some(reason) = self.reasons.get(&flow) { return Ok(*reason); }
        let source = self.flows.node(flow)?.source;
        let source_id = match source {
            super::ProducerFlowSource::Expression(id) => id.source,
            super::ProducerFlowSource::Statement(id) => id.source,
            super::ProducerFlowSource::Stage(id) => id.pipeline.source,
            super::ProducerFlowSource::Comprehension(id) => id.expression.source,
            super::ProducerFlowSource::Binding { identity, .. } => identity.source,
            super::ProducerFlowSource::Parameter { declaration, .. } | super::ProducerFlowSource::DeclarationResult(declaration) => declaration.source,
        };
        let span = self.inputs.origins.get(&flow).copied().unwrap_or_else(|| Span::at(source_id, 0));
        let reason = self.graph.reason(span, None)?;
        self.reasons.insert(flow, reason);
        Ok(reason)
    }

    fn union_effects(&mut self, entry: usize, purpose: UnionPurpose, path: &ProducerPath, close: bool, incoming: Vec<EffectSummary>) -> Result<EffectSummary, InferenceError> {
        self.graph.charge_source_fact_work(incoming.len() as u64 + path.0.len() as u64 + 1)?;
        let mut unique = FxHashSet::default();
        let mut finite = EffectSet::EMPTY;
        for effect in incoming {
            let effect = self.graph.resolved_effect_summary(effect)?;
            match effect {
                EffectSummary::Unknown => return Ok(EffectSummary::Unknown),
                EffectSummary::Closed(bits) => finite.0 |= bits.0,
                _ => { unique.insert(effect); }
            }
        }
        if unique.is_empty() { return Ok(EffectSummary::Closed(finite)); }
        if finite == EffectSet::EMPTY && unique.len() == 1 { return Ok(*unique.iter().next().unwrap()); }
        if finite != EffectSet::EMPTY { unique.insert(EffectSummary::Closed(finite)); }
        let key = (entry, purpose, path.clone(), close);
        if !self.unions.contains_key(&key) {
            let output = EffectSummary::Variable(self.graph.fresh_derived_effect_at(self.inputs.level, None)?);
            self.graph.charge_source_fact_nodes(1)?;
            self.unions.insert(key.clone(), EffectUnion { output, inputs: FxHashSet::default() });
        }
        let output = self.unions[&key].output;
        let reason = self.reason(entry)?;
        for effect in unique {
            if !self.unions[&key].inputs.contains(&effect) {
                self.graph.include_effects(effect, output, reason)?;
                self.unions.get_mut(&key).unwrap().inputs.insert(effect);
            }
        }
        Ok(output)
    }

    fn join_for(&mut self, entry: usize, purpose: UnionPurpose, values: Vec<ProducerEvaluationValue>) -> Result<ProducerEvaluationValue, InferenceError> {
        self.graph.charge_source_fact_work(values.iter().map(value_size).sum::<usize>() as u64 + 1)?;
        let mut result = ProducerEvaluationValue::default();
        let mut effects: BTreeMap<ProducerPath, (Vec<EffectSummary>, Vec<EffectSummary>)> = BTreeMap::new();
        let mut targets = BTreeSet::new();
        for mut value in values {
            value.normalize_pending();
            merge_uncertainty(self.graph, &mut result.pending_paths, &mut result.pending_exclusions, value.pending_paths, value.pending_exclusions)?;
            merge_uncertainty(self.graph, &mut result.opaque_paths, &mut result.opaque_exclusions, value.opaque_paths, value.opaque_exclusions)?;
            for (path, parameters) in value.symbolic_parameters { result.symbolic_parameters.entry(path).or_default().extend(parameters); }
            for (path, roles) in value.profile {
                let inputs = effects.entry(path).or_default(); inputs.0.push(roles.pull); inputs.1.push(roles.close);
            }
            for (path, callables) in value.callable_targets {
                for target in callables {
                    if target.environment.owner != self.graph.owner() { return Err(InferenceError::ForeignHandle); }
                    let key = (path.clone(), target.target, Arc::as_ptr(&target.environment) as usize);
                    if targets.insert(key) { result.callable_targets.entry(path.clone()).or_default().push(target); }
                }
            }
        }
        for (path, (pull, close)) in effects {
            let roles = ProducerEffects { pull: self.union_effects(entry, purpose, &path, false, pull)?, close: self.union_effects(entry, purpose, &path, true, close)? };
            result.profile.insert(path, roles);
        }
        result.pending = result.pending_at(&ProducerPath::default());
        Ok(result)
    }

    fn join(&mut self, entry: usize, values: Vec<ProducerEvaluationValue>) -> Result<ProducerEvaluationValue, InferenceError> {
        self.join_for(entry, UnionPurpose::Result, values)
    }

    fn prefix(&mut self, prefix: &ProducerPath, value: ProducerEvaluationValue) -> Result<ProducerEvaluationValue, InferenceError> {
        self.prefix_payload(prefix, value, false)
    }

    fn prefix_payload(&mut self, prefix: &ProducerPath, mut value: ProducerEvaluationValue, optional: bool) -> Result<ProducerEvaluationValue, InferenceError> {
        value.normalize_pending();
        self.graph.charge_source_fact_work(value_size(&value) as u64 + prefix.0.len() as u64)?;
        for suffix in value.profile.keys().chain(value.callable_targets.keys()).chain(&value.opaque_paths).chain(&value.pending_paths).chain(value.symbolic_parameters.keys()).chain(value.opaque_exclusions.values().chain(value.pending_exclusions.values()).flat_map(|paths| paths.iter())) {
            let already_optional = optional && suffix.0.first() == Some(&ProducerPathComponent::OptionalPayload);
            if suffix.0.len() + if already_optional { 0 } else { prefix.0.len() } > self.graph.limits().structural_depth { return Err(InferenceError::Limit("producer path depth")); }
        }
        let path = |mut suffix: ProducerPath| {
            if optional && suffix.0.first() == Some(&ProducerPathComponent::OptionalPayload) { return suffix; }
            let mut path = prefix.0.clone(); path.append(&mut suffix.0); ProducerPath(path)
        };
        let mut result = ProducerEvaluationValue {
            profile: value.profile.into_iter().map(|(suffix, effects)| (path(suffix), effects)).collect(),
            callable_targets: value.callable_targets.into_iter().map(|(suffix, targets)| (path(suffix), targets)).collect(),
            opaque_paths: value.opaque_paths.into_iter().map(path).collect(),
            pending_paths: value.pending_paths.into_iter().map(path).collect(),
            opaque_exclusions: value.opaque_exclusions.into_iter().map(|(origin, excluded)| (path(origin), excluded.into_iter().map(path).collect())).collect(),
            pending_exclusions: value.pending_exclusions.into_iter().map(|(origin, excluded)| (path(origin), excluded.into_iter().map(path).collect())).collect(),
            symbolic_parameters: value.symbolic_parameters.into_iter().map(|(suffix, parameters)| (path(suffix), parameters)).collect(),
            pending: false,
        };
        result.pending = result.pending_at(&ProducerPath::default());
        Ok(result)
    }

    fn project(&mut self, path: &ProducerPath, mut value: ProducerEvaluationValue) -> Result<ProducerEvaluationValue, InferenceError> {
        value.normalize_pending();
        self.graph.charge_source_fact_work(value_size(&value) as u64 + path.0.len() as u64)?;
        if path.0.len() > self.graph.limits().structural_depth { return Err(InferenceError::Limit("producer path depth")); }
        let suffix = |candidate: ProducerPath| candidate.0.strip_prefix(path.0.as_slice()).map(|suffix| ProducerPath(suffix.to_vec()));
        let (opaque_paths, opaque_exclusions) = project_uncertainty(self.graph, path, value.opaque_paths, value.opaque_exclusions)?;
        let (pending_paths, pending_exclusions) = project_uncertainty(self.graph, path, value.pending_paths, value.pending_exclusions)?;
        let mut symbolic_parameters: BTreeMap<ProducerPath, BTreeSet<ProducerEvaluationParameter>> = BTreeMap::new();
        for (output, parameters) in value.symbolic_parameters {
            if let Some(projection) = path.0.strip_prefix(output.0.as_slice()) {
                for mut parameter in parameters {
                    self.graph.charge_source_fact_work(parameter.path.0.len() as u64 + projection.len() as u64 + 1)?;
                    if parameter.path.0.len() + projection.len() > self.graph.limits().structural_depth { return Err(InferenceError::Limit("producer parameter path depth")); }
                    parameter.path.0.extend_from_slice(projection);
                    if !parameter.excluded_paths.iter().any(|excluded| parameter.path.0.starts_with(&excluded.0)) {
                        symbolic_parameters.entry(ProducerPath::default()).or_default().insert(parameter);
                    }
                }
            } else if let Some(output) = suffix(output) { symbolic_parameters.entry(output).or_default().extend(parameters); }
        }
        let mut result = ProducerEvaluationValue {
            profile: value.profile.into_iter().filter_map(|(candidate, effects)| suffix(candidate).map(|path| (path, effects))).collect(),
            callable_targets: value.callable_targets.into_iter().filter_map(|(candidate, targets)| suffix(candidate).map(|path| (path, targets))).collect(),
            opaque_paths, opaque_exclusions, pending_paths, pending_exclusions, symbolic_parameters, pending: false,
        };
        result.pending = result.pending_at(&ProducerPath::default());
        Ok(result)
    }

    fn remove_replaced_path(&mut self, value: &mut ProducerEvaluationValue, path: &ProducerPath) -> Result<(), InferenceError> {
        value.normalize_pending();
        self.graph.charge_source_fact_work(value_size(value) as u64 + path.0.len() as u64 + 1)?;
        if path.0.len() > self.graph.limits().structural_depth { return Err(InferenceError::Limit("producer path depth")); }
        value.profile.retain(|output, _| !output.0.starts_with(&path.0));
        value.callable_targets.retain(|output, _| !output.0.starts_with(&path.0));
        value.symbolic_parameters.retain(|output, _| !output.0.starts_with(&path.0));
        for (output, parameters) in &mut value.symbolic_parameters {
            if let Some(suffix) = path.0.strip_prefix(output.0.as_slice()) {
                let mut updated = BTreeSet::new();
                for mut parameter in std::mem::take(parameters) {
                    self.graph.charge_source_fact_work(parameter.path.0.len() as u64 + suffix.len() as u64 + 1)?;
                    if parameter.path.0.len() + suffix.len() > self.graph.limits().structural_depth { return Err(InferenceError::Limit("producer parameter path depth")); }
                    let mut excluded = parameter.path.0.clone();
                    excluded.extend_from_slice(suffix);
                    parameter.excluded_paths.insert(ProducerPath(excluded));
                    updated.insert(parameter);
                }
                *parameters = updated;
            }
        }
        exclude_uncertainty(path, &mut value.opaque_paths, &mut value.opaque_exclusions);
        exclude_uncertainty(path, &mut value.pending_paths, &mut value.pending_exclusions);
        value.pending = value.pending_at(&ProducerPath::default());
        Ok(())
    }

    fn lexical_environment(&mut self, declaration: DeclarationIdentity, environment: Arc<ProducerEvaluationEnvironment>) -> Result<Option<Arc<ProducerEvaluationEnvironment>>, InferenceError> {
        let Some(enclosing) = self.inputs.declarations.get(&declaration).and_then(|declaration| declaration.enclosing) else { return Ok(None); };
        let mut current = Some(environment);
        while let Some(frame) = current {
            self.graph.charge_source_fact_work(1)?;
            if frame.declaration == Some(enclosing) { return Ok(Some(frame)); }
            current = frame.parent.clone();
        }
        Ok(None)
    }

    fn frame(&mut self, declaration: DeclarationIdentity, parent: Option<Arc<ProducerEvaluationEnvironment>>, parameters: BTreeMap<u32, ProducerEvaluationValue>, effects: Vec<(EffectSummary, EffectSummary)>, requirements: Vec<(RequirementId, RequirementId)>) -> Result<Arc<ProducerEvaluationEnvironment>, InferenceError> {
        let cost = parameters.values().map(value_size).sum::<usize>() + (effects.len() + requirements.len()) * 2 + 1;
        self.graph.charge_source_fact_work(cost as u64)?;
        let key = EnvironmentKey { declaration, parent: parent.as_ref().map_or(0, |parent| Arc::as_ptr(parent) as usize), parameters, effects, requirements };
        if let Some(frame) = self.environments.get(&key) { return Ok(frame.clone()); }
        self.graph.charge_source_fact_nodes(1)?;
        self.graph.charge_source_fact_edges(cost as u64)?;
        let frame = Arc::new(ProducerEvaluationEnvironment { owner: self.graph.owner(), declaration: Some(declaration), parameters: key.parameters.clone(), parent, effects: key.effects.clone(), requirements: key.requirements.clone() });
        self.environments.insert(key, frame.clone());
        Ok(frame)
    }

    fn contextual_requirement(&mut self, source: RequirementId, environment: &Arc<ProducerEvaluationEnvironment>) -> Result<RequirementId, InferenceError> {
        self.graph.requirement_template(source)?;
        let mut frame = Some(environment);
        while let Some(current) = frame {
            self.graph.charge_source_fact_work(current.requirements.len() as u64 + 1)?;
            let mut instances = current.requirements.iter().filter_map(|(original, instance)| (*original == source).then_some(*instance));
            if let Some(instance) = instances.next() {
                if instances.any(|other| other != instance) { return Err(InferenceError::Boundary("producer invocation has ambiguous instance ancestry")); }
                self.graph.requirement_template(instance)?;
                return Ok(instance);
            }
            frame = current.parent.as_ref();
        }
        Ok(source)
    }

    fn invocation_requirement(&mut self, call: EvaluationCall, environment: &Arc<ProducerEvaluationEnvironment>) -> Result<Option<RequirementId>, InferenceError> {
        if let EvaluationCall::Stage(stage) = call
            && let Some(&(operation, slot)) = self.inputs.stage_invocation_projections.get(&stage) {
            let operation = self.contextual_requirement(operation, environment)?;
            return self.graph.candidate_callback_invocation(operation, slot);
        }
        let source = match call {
            EvaluationCall::Expression(call) => self.inputs.invocation_requirements.get(&call),
            EvaluationCall::Stage(stage) => self.inputs.stage_invocation_requirements.get(&stage),
        }.copied();
        source.map(|source| self.contextual_requirement(source, environment)).transpose()
    }

    fn binding(&mut self, call: EvaluationCall, environment: &Arc<ProducerEvaluationEnvironment>, target: Option<ProducerEvaluationCallableTarget>) -> Result<Option<EvaluationBinding>, InferenceError> {
        let has_invocation = match call {
            EvaluationCall::Expression(call) => self.inputs.invocation_requirements.contains_key(&call),
            EvaluationCall::Stage(stage) => self.inputs.stage_invocation_requirements.contains_key(&stage) || self.inputs.stage_invocation_projections.contains_key(&stage),
        };
        if has_invocation {
            let Some(instance) = self.invocation_requirement(call, environment)? else { return Ok(None); };
            if self.graph.invocation_evidence(instance)?.is_some() {
                use crate::sema::inference::{CallableAuthority, InvocationPlan};
                let RequirementTemplate::CallableInvocation { call: actual_call } = self.graph.requirement_template(instance)? else { return Err(InferenceError::InvalidScheme); };
                let branch_count = match &self.graph.invocation_evidence(instance)?.unwrap().plan {
                    InvocationPlan::Unique { .. } => 1,
                    InvocationPlan::All { branches } => branches.len(),
                };
                self.graph.charge_source_fact_work(branch_count as u64 + 1)?;
                let native_origin = match target {
                    Some(ProducerEvaluationCallableTarget::Native(authority)) => Some(self.graph.native_authority_origin(authority)?),
                    _ => None,
                };
                let selected = match &self.graph.invocation_evidence(instance)?.unwrap().plan {
                    InvocationPlan::Unique { .. } => None,
                    InvocationPlan::All { branches } => {
                        let Some(target) = target else { return Ok(None); };
                        let mut selected = None;
                        for (index, branch) in branches.iter().enumerate() {
                            let matches = match (target, branch.authority) {
                                (ProducerEvaluationCallableTarget::Declaration { origin: Some(origin), .. }, CallableAuthority::User { origin: actual, .. }) => origin == actual,
                                (ProducerEvaluationCallableTarget::Native(_), CallableAuthority::Native { authority }) => Some(self.graph.native_authority_origin(authority)?) == native_origin,
                                _ => false,
                            };
                            if matches {
                                if selected.replace(index).is_some() { return Err(InferenceError::InvalidScheme); }
                            }
                        }
                        Some(selected.ok_or(InferenceError::Boundary("producer callable has no checked conditional branch"))?)
                    }
                };
                let plan = match &self.graph.invocation_evidence(instance)?.unwrap().plan {
                    InvocationPlan::Unique { binding, .. } => binding,
                    InvocationPlan::All { branches } => &branches[selected.unwrap()].binding,
                };
                let cost = plan.supplied_slots.len() + plan.default_slots.len() + plan.dynamic.as_ref().map_or(0, dynamic_work) + self.graph.invocation_call(actual_call)?.arguments.len() + 1;
                self.graph.charge_source_fact_work(cost as u64)?;
                let kinds = self.graph.invocation_call(actual_call)?.arguments.iter().map(|argument| argument.kind).collect();
                let plan = match &self.graph.invocation_evidence(instance)?.unwrap().plan {
                    InvocationPlan::Unique { binding, .. } => binding,
                    InvocationPlan::All { branches } => &branches[selected.unwrap()].binding,
                };
                return Ok(Some(EvaluationBinding { plan: CallBinding { supplied_slots: plan.supplied_slots.clone(), default_slots: plan.default_slots.clone(), rest_slot: plan.rest_slot, dynamic: plan.dynamic.clone() }, kinds }));
            }
            return Ok(None);
        }
        let binding = match call { EvaluationCall::Expression(call) => self.inputs.call_bindings.get(&call), EvaluationCall::Stage(stage) => self.inputs.stage_bindings.get(&stage) };
        if let Some(binding) = binding {
            self.graph.charge_source_fact_work((binding.supplied_slots.len() + binding.default_slots.len() + 1) as u64)?;
            if binding.dynamic.is_some() { return Ok(None); }
            Ok(Some(EvaluationBinding { plan: binding.clone(), kinds: vec![InvocationArgumentKind::Positional; binding.supplied_slots.len()] }))
        } else { Ok(None) }
    }

    fn apply_native(&mut self, entry: usize, call: EvaluationCall, target: NativeAuthority, arguments: &[ProducerEvaluationValue], transfers: &[ProducerArgumentTransfer], caller: &Arc<ProducerEvaluationEnvironment>) -> Result<ProducerEvaluationValue, InferenceError> {
        use crate::sema::registry_graph::{RegistryLifecycle, RegistryProducerComponent, RegistryProducerInput, RegistryProducerTransferPlan};
        use crate::sema::inference::ProducerRole;
        let origin = self.graph.native_authority_origin(target)?;
        let Some(requirement) = self.invocation_requirement(call, caller)? else { return Ok(ProducerEvaluationValue { pending: true, ..Default::default() }); };
        let Some(evidence) = self.graph.invocation_evidence(requirement)? else { return Ok(ProducerEvaluationValue { pending: true, ..Default::default() }); };
        self.graph.charge_source_fact_work(evidence.native_alternatives.len() as u64 + 1)?;
        let mut alternatives = Vec::new();
        for alternative in self.graph.invocation_evidence(requirement)?.unwrap().native_alternatives.iter().copied() {
            if self.graph.native_authority_origin(alternative.authority)? == origin { alternatives.push(alternative); }
        }
        if alternatives.is_empty() { return Err(InferenceError::Boundary("native producer target differs from its checked invocation")); }
        let registry = self.registry.ok_or(InferenceError::Boundary("native producer invocation has no canonical registry"))?;
        let path = |components: &[RegistryProducerComponent]| ProducerPath(components.iter().map(|component| match component {
            RegistryProducerComponent::ListItem => ProducerPathComponent::ListItem,
            RegistryProducerComponent::MapKey => ProducerPathComponent::MapKey,
            RegistryProducerComponent::MapValue => ProducerPathComponent::MapValue,
            RegistryProducerComponent::OptionalPayload => ProducerPathComponent::OptionalPayload,
            RegistryProducerComponent::ResultSuccess => ProducerPathComponent::ResultSuccess,
            RegistryProducerComponent::ResultError => ProducerPathComponent::ResultError,
            RegistryProducerComponent::RecordField(name) => ProducerPathComponent::RecordField(*name),
        }).collect());
        let mut results = Vec::new();
        for alternative in alternatives {
            let selected = self.graph.candidate_evidence(alternative.operation)?;
            let member = if let Some(selected) = selected {
                self.graph.native_authority_member(alternative.authority, selected.candidate)?
            } else if let NativeAuthority::Single(member) = alternative.authority {
                member
            } else {
                results.push(ProducerEvaluationValue { pending: true, ..Default::default() });
                continue;
            };
            let contract = self.graph.native_contract(member)?;
            let candidate = contract.candidate;
            let family = contract.family;
            let signature = contract.instance.ty;
            let RequirementTemplate::Operation { family: actual_family, call: operation } = self.graph.requirement_template(alternative.operation)? else { return Err(InferenceError::InvalidScheme); };
            let operation_call = self.graph.operation_call(operation)?;
            if actual_family != family || operation_call.mono_authority != Some(alternative.authority) { return Err(InferenceError::Boundary("native producer operation lost its monomorphic contract")); }
            if let Some(selected) = self.graph.candidate_evidence(alternative.operation)? {
                if selected.candidate != candidate || self.graph.resolved(selected.signature)? != self.graph.resolved(signature)? { return Err(InferenceError::InvalidScheme); }
            } else if self.graph.family(family)? != [candidate] {
                results.push(ProducerEvaluationValue { pending: true, ..Default::default() });
                continue;
            }
            let metadata = registry.metadata(self.graph, candidate)?;
            let mut value = ProducerEvaluationValue::default();
            if matches!(metadata.lifecycle, RegistryLifecycle::ResultProducer { .. }) {
                self.graph.charge_source_fact_work(3)?;
                let role = |role| {
                    if self.graph.candidate_evidence(alternative.operation)?.is_some() {
                        self.graph.candidate_output_effect(alternative.operation, role)?.ok_or(InferenceError::InvalidScheme)
                    } else {
                        self.graph.operation_call(operation)?.output_effect_bindings.iter().find_map(|(actual, effect)| (*actual == role).then_some(*effect)).ok_or(InferenceError::InvalidScheme)
                    }
                };
                let pull = role(ProducerRole::Pull)?;
                let close = role(ProducerRole::Close)?;
                let effects = ProducerEffects { pull: self.substitute_effect(pull, caller)?, close: self.substitute_effect(close, caller)? };
                value.profile.insert(ProducerPath(vec![ProducerPathComponent::ResultSuccess]), effects);
            }
            results.push(value);
            match &metadata.producer_transfer {
                RegistryProducerTransferPlan::Empty => {}
                RegistryProducerTransferPlan::Opaque => results.push(ProducerEvaluationValue { opaque_paths: BTreeSet::from([ProducerPath::default()]), ..Default::default() }),
                RegistryProducerTransferPlan::Transfers(native_transfers) => {
                    self.graph.charge_source_fact_work(native_transfers.len() as u64 + 1)?;
                    for native in native_transfers.iter() {
                        let RegistryProducerInput::Argument(slot) = native.input else { return Err(InferenceError::Boundary("native callable receiver has no checked producer binding")); };
                        let depth = native.input_path.len().max(native.output_path.len());
                        if depth > self.graph.limits().structural_depth { return Err(InferenceError::Limit("native producer path depth")); }
                        self.graph.charge_source_fact_work((native.input_path.len() + native.output_path.len() + transfers.len() + 1) as u64)?;
                        let input_path = path(&native.input_path);
                        let output_path = path(&native.output_path);
                        for transfer in transfers.iter().filter(|transfer| transfer.slot == slot as usize) {
                            let argument = arguments.get(transfer.argument).ok_or(InferenceError::InvalidScheme)?;
                            self.graph.charge_source_fact_work(value_size(argument) as u64 + 1)?;
                            let argument = if transfer.splice { self.project(&ProducerPath(vec![ProducerPathComponent::ListItem]), argument.clone())? } else { argument.clone() };
                            let argument = if transfer.rest { self.prefix(&ProducerPath(vec![ProducerPathComponent::ListItem]), argument)? } else { argument };
                            let argument = self.project(&input_path, argument)?;
                            results.push(self.prefix(&output_path, argument)?);
                        }
                    }
                }
            }
        }
        self.join(entry, results)
    }

    fn apply(&mut self, entry: usize, call: EvaluationCall, callee: ProducerEvaluationValue, arguments: Vec<ProducerEvaluationValue>, caller: Arc<ProducerEvaluationEnvironment>) -> Result<ProducerEvaluationValue, InferenceError> {
        let mut results = Vec::new();
        let mut symbolic = self.project(&ProducerPath(vec![ProducerPathComponent::CallableResult]), callee.clone())?;
        if callee.pending_at(&ProducerPath::default()) { symbolic.mark_pending(); }
        // An opaque callable can return a producer whose permissions remain
        // unknown. Uncertainty cannot be erased by an empty result profile.
        if callee.opaque_at(&ProducerPath::default()) { symbolic.opaque_paths.insert(ProducerPath::default()); }
        results.push(symbolic);
        let targets = callee.callable_targets.get(&ProducerPath::default()).cloned().unwrap_or_default();
        self.graph.charge_source_fact_work(targets.len() as u64 + 1)?;
        if targets.is_empty() && self.binding(call, &caller, None)?.is_none() { results[0].mark_pending(); }
        for target in targets {
            if target.environment.owner != self.graph.owner() { return Err(InferenceError::ForeignHandle); }
            let Some(binding) = self.binding(call, &caller, Some(target.target))? else { results[0].mark_pending(); continue; };
            if binding.kinds.len() != arguments.len() { return Err(InferenceError::Boundary("producer arguments disagree with call binding evidence")); }
            let transfers = producer_argument_transfers(self.graph, &binding.plan, &binding.kinds)?;
            let binding = binding.plan;
            self.graph.charge_source_fact_work(transfers.len() as u64 + binding.default_slots.len() as u64)?;
            let declaration_id = match target.target {
                ProducerEvaluationCallableTarget::Declaration { declaration, .. } => declaration,
                ProducerEvaluationCallableTarget::Native(contract) => {
                    results.push(self.apply_native(entry, call, contract, &arguments, &transfers, &caller)?);
                    continue;
                }
            };
            let Some(declaration) = self.inputs.declarations.get(&declaration_id) else { results[0].mark_pending(); continue; };
            let Some(result) = declaration.result else { results[0].mark_pending(); continue; };
            let count = declaration.parameters.len();
            let defaults = declaration.defaults.clone();
            let mut bound: BTreeMap<u32, Vec<ProducerEvaluationValue>> = BTreeMap::new();
            for transfer in &transfers {
                let argument = &arguments[transfer.argument];
                let slot = transfer.slot;
                self.graph.charge_source_fact_work(value_size(argument) as u64 + 1)?;
                if slot >= count { return Err(InferenceError::Boundary("producer parameter slot is outside its declaration")); }
                if binding.dynamic.is_none() && !transfer.rest && bound.contains_key(&(slot as u32)) { return Err(InferenceError::Boundary("producer parameter slot is supplied twice")); }
                let argument = if transfer.splice { self.project(&ProducerPath(vec![ProducerPathComponent::ListItem]), argument.clone())? } else { argument.clone() };
                let argument = if transfer.rest { self.prefix(&ProducerPath(vec![ProducerPathComponent::ListItem]), argument)? } else { argument };
                bound.entry(slot as u32).or_default().push(argument);
            }
            let mut parameters = BTreeMap::new();
            for (slot, values) in bound { parameters.insert(slot, self.join_for(entry, UnionPurpose::Parameter(declaration_id, slot), values)?); }
            if let Some(rest) = binding.rest_slot {
                if rest >= count { return Err(InferenceError::Boundary("producer rest slot is outside its declaration")); }
                parameters.entry(rest as u32).or_default();
            }
            let parent = self.lexical_environment(declaration_id, target.environment.clone())?;
            if declaration.enclosing.is_some() && parent.is_none() { results[0].mark_pending(); continue; }
            let substitutions = match call { EvaluationCall::Expression(call) => self.inputs.call_effect_substitutions.get(&call), EvaluationCall::Stage(stage) => self.inputs.stage_effect_substitutions.get(&stage) }.cloned().unwrap_or_default();
            let mut effects = Vec::with_capacity(substitutions.len());
            for (formal, actual) in substitutions { effects.push((formal, self.substitute_effect(actual, &caller)?)); }
            let correspondences = match call { EvaluationCall::Expression(call) => self.inputs.call_requirement_origins.get(&call), EvaluationCall::Stage(stage) => self.inputs.stage_requirement_origins.get(&stage) }.cloned().unwrap_or_default();
            self.graph.charge_source_fact_work(correspondences.len() as u64)?;
            let mut requirements = Vec::with_capacity(correspondences.len());
            for (source, instance) in correspondences {
                self.graph.requirement_template(source)?;
                requirements.push((source, self.contextual_requirement(instance, &caller)?));
            }
            let mut environment = self.frame(declaration_id, parent.clone(), parameters.clone(), effects.clone(), requirements.clone())?;
            let conditional = binding.dynamic.as_ref().map(|dynamic| dynamic.conditional_default_slots.as_slice()).unwrap_or(&[]);
            self.graph.charge_source_fact_work((binding.default_slots.len() + conditional.len()) as u64)?;
            let mut definite = binding.default_slots.iter().copied().peekable();
            let mut conditional = conditional.iter().copied().peekable();
            while definite.peek().is_some() || conditional.peek().is_some() {
                let (slot, conditional) = if definite.peek().is_some_and(|slot| conditional.peek().is_none_or(|other| slot < other)) {
                    (definite.next().unwrap(), false)
                } else { (conditional.next().unwrap(), true) };
                self.graph.charge_source_fact_work(1)?;
                if slot >= count || !conditional && parameters.contains_key(&(slot as u32)) { return Err(InferenceError::Boundary("producer default slots disagree with supplied arguments")); }
                let Some(default) = defaults.get(&(slot as u32)) else { results[0].mark_pending(); continue; };
                let value = self.read(*default, environment, entry)?;
                let value = if let Some(supplied) = parameters.remove(&(slot as u32)) {
                    self.join_for(entry, UnionPurpose::Parameter(declaration_id, slot as u32), vec![supplied, value])?
                } else { value };
                parameters.insert(slot as u32, value);
                environment = self.frame(declaration_id, parent.clone(), parameters.clone(), effects.clone(), requirements.clone())?;
            }
            if parameters.len() != count { results[0].mark_pending(); continue; }
            results.push(self.read(result, environment, entry)?);
        }
        self.join(entry, results)
    }

    fn transfer(&mut self, entry: usize) -> Result<ProducerEvaluationValue, InferenceError> {
        let flow = self.entries[entry].flow;
        let environment = self.entries[entry].environment.clone();
        let node = self.flows.node(flow)?;
        let cost = match &node.kind {
            ProducerFlowKind::Known(profile) => profile.keys().map(|path| 1 + path.0.len()).sum::<usize>(),
            ProducerFlowKind::Aggregate { entries } => entries.iter().map(|entry| 1 + entry.path.0.len()).sum(),
            ProducerFlowKind::RecordUpdate { replacements, .. } => 1 + replacements.iter().map(|entry| 1 + entry.path.0.len()).sum::<usize>(),
            ProducerFlowKind::Join { inputs } => inputs.len(),
            ProducerFlowKind::Apply { arguments, .. } => arguments.len() + 1,
            ProducerFlowKind::StageApply { arguments, .. } => arguments.len() + 1,
            ProducerFlowKind::Addition { .. } => 2,
            ProducerFlowKind::Project { path, .. } => path.0.len() + 1,
            ProducerFlowKind::Operation { alternatives, .. } => alternatives.iter().map(|alternative| 1 + alternative.path.as_ref().map_or(0, |path| path.0.len()) + alternative.transfers.iter().map(|transfer| 1 + transfer.input_path.0.len() + transfer.output_path.0.len()).sum::<usize>()).sum(),
            _ => 1,
        };
        self.graph.charge_source_fact_work(cost as u64 + 1)?;
        let kind = node.kind.clone();
        match kind {
            ProducerFlowKind::OptionalLift { input } => {
                let value = self.read(input, environment, entry)?;
                self.prefix_payload(&ProducerPath(vec![ProducerPathComponent::OptionalPayload]), value, true)
            }
            ProducerFlowKind::Empty => Ok(ProducerEvaluationValue::default()),
            ProducerFlowKind::Opaque => Ok(ProducerEvaluationValue { opaque_paths: BTreeSet::from([ProducerPath::default()]), ..Default::default() }),
            ProducerFlowKind::Known(mut profile) => {
                for roles in profile.values_mut() { roles.pull = self.substitute_effect(roles.pull, &environment)?; roles.close = self.substitute_effect(roles.close, &environment)?; }
                Ok(ProducerEvaluationValue { profile, ..Default::default() })
            }
            ProducerFlowKind::Addition { requirement, left, right } => {
                let requirement = self.contextual_requirement(requirement, &environment)?;
                match self.graph.discharge(requirement)?.map(|evidence| evidence.operation) {
                    Some(crate::sema::inference::SealedOperation::AddList) => {
                        let left = self.read(left, environment.clone(), entry)?;
                        let right = self.read(right, environment, entry)?;
                        self.join(entry, vec![left, right])
                    }
                    Some(_) => Ok(ProducerEvaluationValue::default()),
                    None => Ok(ProducerEvaluationValue { pending: true, ..Default::default() }),
                }
            }
            ProducerFlowKind::Operation { requirement, alternatives, mut outputs } => {
                outputs.pull = self.substitute_effect(outputs.pull, &environment)?;
                outputs.close = self.substitute_effect(outputs.close, &environment)?;
                let requirement = self.contextual_requirement(requirement, &environment)?;
                let evidence = self.graph.candidate_evidence(requirement)?;
                let alternative = if let Some(evidence) = evidence {
                    Some(alternatives.iter().find(|alternative| alternative.candidate == evidence.candidate).ok_or(InferenceError::InvalidScheme)?)
                } else if let Some(first) = alternatives.first() {
                    alternatives.iter().all(|alternative| alternative.path == first.path && alternative.transfers == first.transfers && alternative.opaque == first.opaque).then_some(first)
                } else { None };
                let Some(alternative) = alternative else { return Ok(ProducerEvaluationValue { pending: true, ..Default::default() }); };
                let mut values = vec![ProducerEvaluationValue { profile: alternative.path.clone().into_iter().map(|path| (path, outputs)).collect(), opaque_paths: if alternative.opaque { BTreeSet::from([ProducerPath::default()]) } else { BTreeSet::new() }, ..Default::default() }];
                for transfer in &alternative.transfers {
                    let value = self.read(transfer.input, environment.clone(), entry)?;
                    let value = self.project(&transfer.input_path, value)?;
                    values.push(self.prefix(&transfer.output_path, value)?);
                }
                self.join(entry, values)
            }
            ProducerFlowKind::Parameter { declaration, index } => {
                let mut frame = Some(environment);
                while let Some(current) = frame {
                    self.graph.charge_source_fact_work(1)?;
                    if current.declaration == Some(declaration) {
                        if let Some(value) = current.parameters.get(&index) { self.graph.charge_source_fact_work(value_size(value) as u64)?; return Ok(value.clone()); }
                        return Ok(ProducerEvaluationValue { pending: true, ..Default::default() });
                    }
                    frame = current.parent.clone();
                }
                let mut value = if let Some(value) = self.inputs.parameters.get(&(declaration, index)) { self.graph.charge_source_fact_work(value_size(value) as u64)?; value.clone() } else { ProducerEvaluationValue::default() };
                if self.inputs.declarations.contains_key(&declaration) {
                    value.symbolic_parameters.entry(ProducerPath::default()).or_default().insert(ProducerEvaluationParameter { declaration, index, path: ProducerPath::default(), excluded_paths: BTreeSet::new() });
                } else { value.mark_pending(); }
                Ok(value)
            }
            ProducerFlowKind::CapturedBinding { input, .. } => self.read(input, environment, entry),
            ProducerFlowKind::Project { input, path } => { let value = self.read(input, environment, entry)?; self.project(&path, value) }
            ProducerFlowKind::Aggregate { entries } => {
                let mut values = Vec::with_capacity(entries.len());
                for field in entries { let value = self.read(field.input, environment.clone(), entry)?; values.push(self.prefix(&field.path, value)?); }
                self.join(entry, values)
            }
            ProducerFlowKind::RecordUpdate { base, replacements } => {
                let mut value = self.read(base, environment.clone(), entry)?;
                for field in replacements {
                    self.remove_replaced_path(&mut value, &field.path)?;
                    let replacement = self.read(field.input, environment.clone(), entry)?;
                    let replacement = self.prefix(&field.path, replacement)?;
                    value = self.join(entry, vec![value, replacement])?;
                }
                Ok(value)
            }
            ProducerFlowKind::Join { inputs } => {
                let mut values = Vec::with_capacity(inputs.len());
                for input in inputs { values.push(self.read(input, environment.clone(), entry)?); }
                self.join(entry, values)
            }
            ProducerFlowKind::Callable { declaration, origin } => {
                let lexical = self.lexical_environment(declaration, environment.clone())?;
                let pending = self.inputs.declarations.get(&declaration).is_none_or(|definition| definition.result.is_none() || definition.enclosing.is_some() && lexical.is_none());
                let environment = if self.inputs.declarations.get(&declaration).is_some_and(|definition| definition.enclosing.is_none()) { self.root_environment.clone() } else { lexical.unwrap_or(environment) };
                let target = ProducerEvaluationCallable { target: ProducerEvaluationCallableTarget::Declaration { declaration, origin }, environment };
                Ok(ProducerEvaluationValue { callable_targets: BTreeMap::from([(ProducerPath::default(), vec![target])]), pending, ..Default::default() })
            }
            ProducerFlowKind::NativeCallable { authority } => {
                self.graph.native_authority_signature(authority)?;
                let target = ProducerEvaluationCallable { target: ProducerEvaluationCallableTarget::Native(authority), environment };
                Ok(ProducerEvaluationValue { callable_targets: BTreeMap::from([(ProducerPath::default(), vec![target])]), ..Default::default() })
            }
            ProducerFlowKind::Apply { call, callee, arguments } => {
                let callee = self.read(callee, environment.clone(), entry)?;
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments { values.push(self.read(argument, environment.clone(), entry)?); }
                self.apply(entry, EvaluationCall::Expression(call), callee, values, environment)
            }
            ProducerFlowKind::StageApply { stage, callee, arguments } => {
                let callee = self.read(callee, environment.clone(), entry)?;
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments { values.push(self.read(argument, environment.clone(), entry)?); }
                self.apply(entry, EvaluationCall::Stage(stage), callee, values, environment)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{ProducerEffects, ProducerFlowGraph, ProducerFlowKind, ProducerFlowSource, ProducerFlowField, ProducerPathComponent};
    use crate::sema::inference::{EffectSet, InferenceContext, InferenceError, Limits};
    use crate::source::SourceId;
    use crate::syntax::arena::{ExprId, FunctionDefId};

    fn declaration(index: usize) -> DeclarationIdentity {
        DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(index) }
    }
    fn call(index: usize) -> ExpressionIdentity {
        ExpressionIdentity { source: SourceId::new(0), namespace: None, expression: ExprId::from_index(index) }
    }
    fn push(graph: &mut InferenceContext, flows: &mut ProducerFlowGraph, kind: ProducerFlowKind) -> ProducerFlowId {
        let index = flows.nodes().len();
        flows.push_fixture(graph, ProducerFlowSource::Expression(call(index)), kind).unwrap()
    }
    fn known(pull: EffectSet, close: EffectSet) -> ProducerProfile {
        BTreeMap::from([(ProducerPath::default(), ProducerEffects { pull: EffectSummary::Closed(pull), close: EffectSummary::Closed(close) })])
    }
    fn binding(slots: &[usize]) -> CallBinding {
        CallBinding { supplied_slots: slots.to_vec(), default_slots: vec![], rest_slot: None, dynamic: None }
    }

    #[test]
    fn native_producer_uses_checked_roles_and_rejects_an_unrelated_same_candidate_value() {
        use crate::sema::inference::{CallableAuthority, CallableDomain, InvocationCall, TypeNode};
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let mut graph = InferenceContext::default();
        let mut registry = crate::sema::registry_graph::RegistryGraph::default();
        let span = Span::at(SourceId::new(0), 0);
        let reason = graph.reason(span, None).unwrap();
        let family = registry.module_family(&mut graph, "process", "list", span).unwrap();
        let candidate = graph.family(family).unwrap()[0];
        let scheme = graph.candidate(candidate).unwrap().scheme;
        let instance = graph.instantiate(scheme, 1, reason).unwrap();
        let native = graph.native_callable(candidate, family, instance).unwrap();
        let TypeNode::NativeCallable(wrapper) = graph.node(native).unwrap() else { panic!() };
        let CallableAuthority::Native { authority: NativeAuthority::Single(contract) } = wrapper.alternatives[0] else { panic!() };
        let signature = graph.callable_signature(native).unwrap();
        let TypeNode::Arrow(arrow) = graph.node(signature).unwrap() else { panic!() };
        let result = arrow.result;
        let requirement = graph.require_callable_invocation(InvocationCall {
            callable: native, arguments: vec![], result,
            effects: EffectSummary::Closed(EffectSet::PROCESS), domain: CallableDomain::AnyCallable,
        }, reason).unwrap();
        graph.solve().unwrap();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let callee = push(&mut graph, &mut flows, ProducerFlowKind::NativeCallable { authority: NativeAuthority::Single(contract) });
        let invocation = call(50);
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: invocation, callee, arguments: vec![] });
        let mut inputs = ProducerEvaluationInputs::default();
        inputs.call_bindings.insert(invocation, binding(&[]));
        let reference = evaluate_producer_flow_with_registry(&mut graph, &flows, callee, &inputs, &registry).unwrap();
        assert!(reference.profile.is_empty(), "a native factory reference does not open a producer");
        assert!(!reference.pending);
        let unresolved = evaluate_producer_flow_with_registry(&mut graph, &flows, root, &inputs, &registry).unwrap();
        assert!(unresolved.pending, "producer roles require the checked invocation");
        inputs.invocation_requirements.insert(invocation, requirement);
        let instantiations = graph.counters().instantiations;
        let value = evaluate_producer_flow_with_registry(&mut graph, &flows, root, &inputs, &registry).unwrap();
        assert!(!value.pending);
        assert_eq!(value.profile, BTreeMap::from([(
            ProducerPath(vec![ProducerPathComponent::ResultSuccess]),
            ProducerEffects { pull: EffectSummary::Closed(EffectSet::PROCESS), close: EffectSummary::Closed(EffectSet::EMPTY) },
        )]));
        assert_eq!(graph.counters().instantiations, instantiations, "producer evaluation consumes the existing native instance");
        let unrelated = graph.instantiate(scheme, 1, reason).unwrap();
        let unrelated = graph.native_callable(candidate, family, unrelated).unwrap();
        let TypeNode::NativeCallable(wrapper) = graph.node(unrelated).unwrap() else { panic!() };
        let CallableAuthority::Native { authority: NativeAuthority::Single(unrelated) } = wrapper.alternatives[0] else { panic!() };
        let unrelated_callee = push(&mut graph, &mut flows, ProducerFlowKind::NativeCallable { authority: NativeAuthority::Single(unrelated) });
        let wrong = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: invocation, callee: unrelated_callee, arguments: vec![] });
        assert!(matches!(evaluate_producer_flow_with_registry(&mut graph, &flows, wrong, &inputs, &registry), Err(InferenceError::Boundary(_))));
    }

    #[test]
    fn native_family_producer_uses_selected_member_ports_and_original_invocation_mask() {
        use crate::sema::inference::{Atom, CallableAuthority, CallableDomain, InvocationArgument, InvocationCall, InvocationArgumentKind, TypeNode};
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        for name in ["ports", "threads"] {
            let mut graph = InferenceContext::default();
            let mut registry = crate::sema::registry_graph::RegistryGraph::default();
            let span = Span::at(SourceId::new(0), 0);
            let reason = graph.reason(span, None).unwrap();
            let family = registry.module_family(&mut graph, "process", name, span).unwrap();
            let candidates = graph.family(family).unwrap().to_vec();
            let mut members = Vec::new();
            for candidate in &candidates {
                let scheme = graph.candidate(*candidate).unwrap().scheme;
                members.push((*candidate, graph.instantiate(scheme, 1, reason).unwrap()));
            }
            let native = graph.native_family_callable(family, members, reason).unwrap();
            let TypeNode::NativeCallable(wrapper) = graph.node(native).unwrap() else { panic!() };
            let CallableAuthority::Native { authority: NativeAuthority::Family(contract) } = wrapper.alternatives[0] else { panic!() };
            let authority = NativeAuthority::Family(contract);
            assert!(matches!(graph.node(graph.callable_signature(native).unwrap()).unwrap(), TypeNode::CallableChoice(_)));
            let mut flows = ProducerFlowGraph::new(graph.owner());
            let callee = push(&mut graph, &mut flows, ProducerFlowKind::NativeCallable { authority });
            let int = graph.atom(Atom::Int).unwrap();
            let instantiations = graph.counters().instantiations;
            for supplied in [0, 1] {
                let arguments = if supplied == 0 { vec![] } else { vec![InvocationArgument { kind: InvocationArgumentKind::Named(crate::symbol::Name::intern("pid")), ty: int }] };
                let result = graph.fresh(0, span).unwrap();
                let requirement = graph.require_callable_invocation(InvocationCall {
                    callable: native, arguments, result,
                    effects: EffectSummary::Closed(EffectSet::PROCESS), domain: CallableDomain::AnyCallable,
                }, reason).unwrap();
                graph.solve().unwrap();
                let invocation = graph.invocation_evidence(requirement).unwrap().unwrap();
                let child = invocation.native_alternatives[0].operation;
                let member = graph.candidate_evidence(child).unwrap().unwrap();
                assert_eq!(member.actual_arguments, if supplied == 0 { vec![] } else { vec![Some(int)] });
                let invocation = call(70 + supplied);
                let arguments = if supplied == 0 { vec![] } else { vec![push(&mut graph, &mut flows, ProducerFlowKind::Empty)] };
                let root = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: invocation, callee, arguments });
                let mut inputs = ProducerEvaluationInputs::default();
                inputs.invocation_requirements.insert(invocation, requirement);
                let value = evaluate_producer_flow_with_registry(&mut graph, &flows, root, &inputs, &registry).unwrap();
                assert!(!value.pending);
                assert_eq!(value.profile, BTreeMap::from([(
                    ProducerPath(vec![ProducerPathComponent::ResultSuccess]),
                    ProducerEffects { pull: EffectSummary::Closed(if name == "threads" { EffectSet::PROCESS } else { EffectSet::EMPTY }), close: EffectSummary::Closed(EffectSet::EMPTY) },
                )]));
                assert!(graph.operation_call(match graph.requirement_template(child).unwrap() {
                    RequirementTemplate::Operation { call, .. } => call,
                    _ => panic!(),
                }).unwrap().output_effect_bindings.is_empty(), "selected ports come from the retained certificate rather than a mutated call payload");
            }
            assert_eq!(graph.counters().instantiations, instantiations);
        }
    }

    #[test]
    fn conditional_user_producers_use_each_branch_default_plan_and_exact_origin() {
        use crate::sema::inference::{Arrow, Atom, CallableDomain, CallableKind, InvocationArgument, InvocationArgumentKind, InvocationCall, InvocationPlan, Parameter};
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let mut graph = InferenceContext::default();
        let span = Span::at(SourceId::new(0), 0);
        let reason = graph.reason(span, None).unwrap();
        let item = graph.atom(Atom::Int).unwrap();
        let stream = graph.stream(item).unwrap();
        let parameter = |name, defaulted| Parameter { label: crate::symbol::Name::intern(name), ty: stream, defaulted, rest: false };
        let first = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![parameter("input", false)], result: stream, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let fallback = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![parameter("source", false), parameter("fallback", true)], result: stream, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let joined = graph.join_callable_values(first, fallback, 0, reason).unwrap();
        let requirement = graph.require_callable_invocation(InvocationCall {
            callable: joined, arguments: vec![InvocationArgument { kind: InvocationArgumentKind::Positional, ty: stream }], result: stream,
            effects: EffectSummary::Closed(EffectSet::EMPTY), domain: CallableDomain::Pure,
        }, reason).unwrap();
        graph.solve().unwrap();
        let InvocationPlan::All { branches } = &graph.invocation_evidence(requirement).unwrap().unwrap().plan else { panic!("a conditional call retains all possible plans") };
        assert_eq!(branches.len(), 2);
        assert_eq!(branches.iter().map(|branch| branch.binding.default_slots.clone()).collect::<BTreeSet<_>>(), BTreeSet::from([vec![], vec![1]]));
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let first_owner = declaration(0);
        let fallback_owner = declaration(1);
        let supplied_parameter = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: first_owner, index: 0 });
        let fallback_parameter = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: fallback_owner, index: 1 });
        let unused_parameter = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: fallback_owner, index: 0 });
        let default = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::EMPTY, EffectSet::ENV)));
        let supplied = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::TIME, EffectSet::EMPTY)));
        let first_flow = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: first_owner, origin: Some(first) });
        let fallback_flow = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: fallback_owner, origin: Some(fallback) });
        let mut inputs = ProducerEvaluationInputs::default();
        inputs.declarations.insert(first_owner, ProducerEvaluationDeclaration { parameters: vec![supplied_parameter], result: Some(supplied_parameter), ..Default::default() });
        inputs.declarations.insert(fallback_owner, ProducerEvaluationDeclaration { parameters: vec![unused_parameter, fallback_parameter], result: Some(fallback_parameter), defaults: BTreeMap::from([(1, default)]), ..Default::default() });
        let count = graph.counters().instantiations;
        for (index, inputs_order) in [vec![first_flow, fallback_flow], vec![fallback_flow, first_flow]].into_iter().enumerate() {
            let callee = push(&mut graph, &mut flows, ProducerFlowKind::Join { inputs: inputs_order });
            let invocation = call(90 + index);
            inputs.invocation_requirements.insert(invocation, requirement);
            let root = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: invocation, callee, arguments: vec![supplied] });
            let value = evaluate_producer_flow(&mut graph, &flows, root, &inputs).unwrap();
            assert!(!value.pending);
            assert_eq!(value.profile, known(EffectSet::TIME, EffectSet::ENV));
        }
        assert_eq!(graph.counters().instantiations, count);
        assert!(inputs.parameters.is_empty(), "branch calls never train declaration-owned formals");
        let foreign_origin = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![parameter("input", false)], result: stream, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let callee = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: first_owner, origin: Some(foreign_origin) });
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(90), callee, arguments: vec![supplied] });
        assert!(matches!(evaluate_producer_flow(&mut graph, &flows, root, &inputs), Err(InferenceError::Boundary(_))));
    }

    #[test]
    fn exact_parameter_slots_transfer_equal_typed_distinct_producers() {
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let owner = declaration(0);
        let first = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: owner, index: 0 });
        let second = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: owner, index: 1 });
        let callee = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: owner, origin: None });
        let time = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::TIME, EffectSet::EMPTY)));
        let env = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::EMPTY, EffectSet::ENV)));
        let invocation = call(30);
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: invocation, callee, arguments: vec![env, time] });
        let mut inputs = ProducerEvaluationInputs::default();
        inputs.declarations.insert(owner, ProducerEvaluationDeclaration { parameters: vec![first, second], result: Some(first), ..Default::default() });
        inputs.call_bindings.insert(invocation, binding(&[1, 0]));
        let value = evaluate_producer_flow(&mut graph, &flows, root, &inputs).unwrap();
        assert!(!value.pending);
        assert_eq!(value.profile, known(EffectSet::TIME, EffectSet::EMPTY));
        assert!(inputs.parameters.is_empty(), "a call must not train definition-owned formals");
    }

    #[test]
    fn aggregate_projection_keeps_uncertainty_at_its_exact_path() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let unknown = push(&mut graph, &mut flows, ProducerFlowKind::Opaque);
        let known = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::TIME, EffectSet::ENV)));
        let path = |name| ProducerPath(vec![ProducerPathComponent::RecordField(crate::symbol::Name::intern(name))]);
        let aggregate = push(&mut graph, &mut flows, ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: path("uncertain"), input: unknown }, ProducerFlowField { path: path("certain"), input: known }] });
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Project { input: aggregate, path: path("certain") });
        let value = evaluate_producer_flow(&mut graph, &flows, root, &ProducerEvaluationInputs::default()).unwrap();
        assert_eq!(value.profile, super::tests::known(EffectSet::TIME, EffectSet::ENV));
        assert!(value.opaque_paths.is_empty());
        let opaque = push(&mut graph, &mut flows, ProducerFlowKind::Project { input: aggregate, path: path("uncertain") });
        assert_eq!(evaluate_producer_flow(&mut graph, &flows, opaque, &ProducerEvaluationInputs::default()).unwrap().opaque_paths, BTreeSet::from([ProducerPath::default()]));
    }

    #[test]
    fn record_update_replaces_profiles_without_changing_the_base() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let path = ProducerPath(vec![ProducerPathComponent::RecordField(crate::symbol::Name::intern("rows"))]);
        let time = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::TIME, EffectSet::EMPTY)));
        let quiet = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::EMPTY, EffectSet::ENV)));
        let base = push(&mut graph, &mut flows, ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: path.clone(), input: time }] });
        let updated = push(&mut graph, &mut flows, ProducerFlowKind::RecordUpdate { base, replacements: vec![ProducerFlowField { path: path.clone(), input: quiet }] });
        let projected = push(&mut graph, &mut flows, ProducerFlowKind::Project { input: updated, path: path.clone() });
        let original = push(&mut graph, &mut flows, ProducerFlowKind::Project { input: base, path });
        let values = evaluate_producer_flows(&mut graph, &flows, &[projected, original], &ProducerEvaluationInputs::default()).unwrap();
        assert_eq!(values[0].profile, known(EffectSet::EMPTY, EffectSet::ENV));
        assert_eq!(values[1].profile, known(EffectSet::TIME, EffectSet::EMPTY));
    }

    #[test]
    fn record_update_excludes_overwritten_formal_and_opaque_subtrees() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let owner = declaration(0);
        let path = ProducerPath(vec![ProducerPathComponent::RecordField(crate::symbol::Name::intern("rows"))]);
        let untouched = ProducerPath(vec![ProducerPathComponent::RecordField(crate::symbol::Name::intern("other"))]);
        let parameter = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: owner, index: 0 });
        let opaque = push(&mut graph, &mut flows, ProducerFlowKind::Opaque);
        let quiet = push(&mut graph, &mut flows, ProducerFlowKind::Empty);
        let mut inputs = ProducerEvaluationInputs::default();
        inputs.declarations.insert(owner, ProducerEvaluationDeclaration { parameters: vec![parameter], ..Default::default() });
        for base in [parameter, opaque] {
            let updated = push(&mut graph, &mut flows, ProducerFlowKind::RecordUpdate { base, replacements: vec![ProducerFlowField { path: path.clone(), input: quiet }] });
            let projected = push(&mut graph, &mut flows, ProducerFlowKind::Project { input: updated, path: path.clone() });
            let other = push(&mut graph, &mut flows, ProducerFlowKind::Project { input: updated, path: untouched.clone() });
            let values = evaluate_producer_flows(&mut graph, &flows, &[projected, other], &inputs).unwrap();
            assert!(values[0].symbolic_parameters.is_empty());
            assert!(values[0].opaque_paths.is_empty());
            assert!(!values[1].symbolic_parameters.is_empty() || !values[1].opaque_paths.is_empty());
        }
    }

    #[test]
    fn record_update_exclusions_do_not_hide_an_unchanged_uncertain_alternative() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let path = ProducerPath(vec![ProducerPathComponent::RecordField(crate::symbol::Name::intern("rows"))]);
        for kind in [ProducerFlowKind::Opaque, ProducerFlowKind::Parameter { declaration: declaration(0), index: 0 }] {
            let unknown = push(&mut graph, &mut flows, kind);
            let quiet = push(&mut graph, &mut flows, ProducerFlowKind::Empty);
            let updated = push(&mut graph, &mut flows, ProducerFlowKind::RecordUpdate { base: unknown, replacements: vec![ProducerFlowField { path: path.clone(), input: quiet }] });
            let alternatives = push(&mut graph, &mut flows, ProducerFlowKind::Join { inputs: vec![updated, unknown] });
            let projected = push(&mut graph, &mut flows, ProducerFlowKind::Project { input: alternatives, path: path.clone() });
            let value = evaluate_producer_flow(&mut graph, &flows, projected, &ProducerEvaluationInputs::default()).unwrap();
            assert!(value.opaque_at(&ProducerPath::default()) || value.pending_at(&ProducerPath::default()));
        }
    }

    #[test]
    fn pending_items_do_not_make_a_materialized_container_root_pending() {
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let unknown = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: declaration(0), index: 0 });
        let path = ProducerPath(vec![ProducerPathComponent::ListItem]);
        let list = push(&mut graph, &mut flows, ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: path.clone(), input: unknown }] });
        let item = push(&mut graph, &mut flows, ProducerFlowKind::Project { input: list, path });
        let values = evaluate_producer_flows(&mut graph, &flows, &[list, item], &ProducerEvaluationInputs::default()).unwrap();
        assert!(!values[0].pending, "the container root has no producer uncertainty");
        assert!(values[1].pending, "projecting the item must preserve its uncertainty");
    }

    #[test]
    fn returned_callable_uses_its_exact_captured_parameter_environment() {
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let outer = declaration(0);
        let inner = declaration(1);
        let parameter = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: outer, index: 0 });
        let closure = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: inner, origin: None });
        let factory = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: outer, origin: None });
        let time = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::TIME, EffectSet::EMPTY)));
        let env = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::EMPTY, EffectSet::ENV)));
        let time_factory = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(20), callee: factory, arguments: vec![time] });
        let env_factory = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(21), callee: factory, arguments: vec![env] });
        let time_result = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(22), callee: time_factory, arguments: vec![] });
        let env_result = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(23), callee: env_factory, arguments: vec![] });
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![ProducerPathComponent::ResultSuccess]), input: time_result }, ProducerFlowField { path: ProducerPath(vec![ProducerPathComponent::ResultError]), input: env_result }] });
        let mut inputs = ProducerEvaluationInputs::default();
        inputs.declarations.insert(outer, ProducerEvaluationDeclaration { parameters: vec![parameter], result: Some(closure), ..Default::default() });
        inputs.declarations.insert(inner, ProducerEvaluationDeclaration { result: Some(parameter), enclosing: Some(outer), ..Default::default() });
        for index in [20, 21] { inputs.call_bindings.insert(call(index), binding(&[0])); }
        for index in [22, 23] { inputs.call_bindings.insert(call(index), binding(&[])); }
        let value = evaluate_producer_flow(&mut graph, &flows, root, &inputs).unwrap();
        assert!(!value.pending);
        assert_eq!(value.profile[&ProducerPath(vec![ProducerPathComponent::ResultSuccess])], known(EffectSet::TIME, EffectSet::EMPTY)[&ProducerPath::default()]);
        assert_eq!(value.profile[&ProducerPath(vec![ProducerPathComponent::ResultError])], known(EffectSet::EMPTY, EffectSet::ENV)[&ProducerPath::default()]);
    }

    #[test]
    fn missing_declaration_results_and_binding_evidence_remain_pending() {
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let callee = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: declaration(0), origin: None });
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(20), callee, arguments: vec![] });
        let mut inputs = ProducerEvaluationInputs::default();
        inputs.declarations.insert(declaration(0), ProducerEvaluationDeclaration::default());
        inputs.call_bindings.insert(call(20), binding(&[]));
        assert!(evaluate_producer_flow(&mut graph, &flows, root, &inputs).unwrap().pending);
    }

    #[test]
    fn generic_callback_invocations_use_each_exact_instance_default_plan() {
        use crate::sema::inference::{Arrow, Atom, CallableDomain, CallableKind, Generalization, InvocationArgument, InvocationArgumentKind, InvocationCall, Parameter, TypeNode};
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let mut graph = InferenceContext::default();
        let span = Span::at(SourceId::new(0), 0);
        let why = graph.reason(span, None).unwrap();
        let item = graph.atom(Atom::Int).unwrap();
        let stream = graph.stream(item).unwrap();
        let callback = graph.fresh(1, span).unwrap();
        let result = graph.fresh(1, span).unwrap();
        let invocation = graph.require_callable_invocation(InvocationCall { callable: callback, arguments: vec![InvocationArgument { kind: InvocationArgumentKind::Positional, ty: stream }], result, effects: EffectSummary::Closed(EffectSet::EMPTY), domain: CallableDomain::Pure }, why).unwrap();
        let parameter = |name, defaulted| Parameter { label: crate::symbol::Name::intern(name), ty: stream, defaulted, rest: false };
        let outer = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { ty: callback, ..parameter("callback", false) }, parameter("value", false)], result, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let scheme = graph.generalize(outer, 0, Generalization::Allowed, &[invocation]).unwrap();
        let mut instances = Vec::new();
        for params in [vec![parameter("input", false)], vec![parameter("source", false), parameter("fallback", true)]] {
            let callback = graph.arrow(Arrow { kind: CallableKind::Pure, params, result: stream, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
            let instance = graph.instantiate(scheme, 0, why).unwrap();
            let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap() else { panic!() };
            let formal = signature.params[0].ty;
            graph.unify(formal, callback, why).unwrap();
            graph.solve().unwrap();
            instances.push(instance.requirements[0]);
        }
        assert!(graph.invocation_evidence(invocation).unwrap().is_none());
        assert_eq!(graph.invocation_evidence(instances[0]).unwrap().unwrap().unique_plan().unwrap().1.default_slots, vec![]);
        assert_eq!(graph.invocation_evidence(instances[1]).unwrap().unwrap().unique_plan().unwrap().1.default_slots, vec![1]);

        let mut flows = ProducerFlowGraph::new(graph.owner());
        let outer = declaration(0);
        let first = declaration(1);
        let fallback = declaration(2);
        let callback = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: outer, index: 0 });
        let value = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: outer, index: 1 });
        let inner = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(10), callee: callback, arguments: vec![value] });
        let factory = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: outer, origin: None });
        let first_parameter = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: first, index: 0 });
        let fallback_input = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: fallback, index: 0 });
        let fallback_parameter = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: fallback, index: 1 });
        let default = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::EMPTY, EffectSet::ENV)));
        let first_callable = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: first, origin: None });
        let fallback_callable = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: fallback, origin: None });
        let time = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::TIME, EffectSet::EMPTY)));
        let first_call = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(20), callee: factory, arguments: vec![first_callable, time] });
        let fallback_call = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(21), callee: factory, arguments: vec![fallback_callable, time] });
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![ProducerPathComponent::ResultSuccess]), input: first_call }, ProducerFlowField { path: ProducerPath(vec![ProducerPathComponent::ResultError]), input: fallback_call }] });
        let mut inputs = ProducerEvaluationInputs::default();
        inputs.declarations.insert(outer, ProducerEvaluationDeclaration { parameters: vec![callback, value], result: Some(inner), ..Default::default() });
        inputs.declarations.insert(first, ProducerEvaluationDeclaration { parameters: vec![first_parameter], result: Some(first_parameter), ..Default::default() });
        inputs.declarations.insert(fallback, ProducerEvaluationDeclaration { parameters: vec![fallback_input, fallback_parameter], result: Some(fallback_parameter), defaults: BTreeMap::from([(1, default)]), ..Default::default() });
        inputs.invocation_requirements.insert(call(10), invocation);
        for index in [20, 21] { inputs.call_bindings.insert(call(index), binding(&[0, 1])); }
        inputs.call_requirement_origins.insert(call(20), vec![(invocation, instances[0])]);
        inputs.call_requirement_origins.insert(call(21), vec![(invocation, instances[1])]);
        let value = evaluate_producer_flow(&mut graph, &flows, root, &inputs).unwrap();
        assert!(!value.pending);
        assert_eq!(value.profile[&ProducerPath(vec![ProducerPathComponent::ResultSuccess])], known(EffectSet::TIME, EffectSet::EMPTY)[&ProducerPath::default()]);
        assert_eq!(value.profile[&ProducerPath(vec![ProducerPathComponent::ResultError])], known(EffectSet::EMPTY, EffectSet::ENV)[&ProducerPath::default()]);
    }

    #[test]
    fn rest_packing_and_ordered_defaults_transfer_real_source_values() {
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let owner = declaration(0);
        let default = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::EMPTY, EffectSet::ENV)));
        let parameter = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: owner, index: 0 });
        let rest = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: owner, index: 1 });
        let result = push(&mut graph, &mut flows, ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![ProducerPathComponent::ResultSuccess]), input: parameter }, ProducerFlowField { path: ProducerPath(vec![ProducerPathComponent::ResultError]), input: rest }] });
        let callee = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: owner, origin: None });
        let time = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::TIME, EffectSet::EMPTY)));
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(20), callee, arguments: vec![time] });
        let mut inputs = ProducerEvaluationInputs::default();
        inputs.declarations.insert(owner, ProducerEvaluationDeclaration { parameters: vec![parameter, rest], result: Some(result), defaults: BTreeMap::from([(0, default)]), ..Default::default() });
        inputs.call_bindings.insert(call(20), CallBinding { supplied_slots: vec![1], default_slots: vec![0], rest_slot: Some(1), dynamic: None });
        let value = evaluate_producer_flow(&mut graph, &flows, root, &inputs).unwrap();
        assert!(!value.pending);
        assert_eq!(value.profile[&ProducerPath(vec![ProducerPathComponent::ResultSuccess])], known(EffectSet::EMPTY, EffectSet::ENV)[&ProducerPath::default()]);
        assert_eq!(value.profile[&ProducerPath(vec![ProducerPathComponent::ResultError, ProducerPathComponent::ListItem])], known(EffectSet::TIME, EffectSet::EMPTY)[&ProducerPath::default()]);
    }

    #[test]
    fn dynamic_splice_preserves_item_permissions_and_conditional_defaults() {
        use crate::sema::inference::{Arrow, Atom, CallableKind, CallableDomain, InvocationCall, InvocationArgument, InvocationArgumentKind, Parameter};
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default();
        let int = graph.atom(Atom::Int).unwrap();
        let stream = graph.stream(int).unwrap(); let list = graph.list(stream).unwrap();
        let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![
            Parameter { label: crate::symbol::Name::intern("first"), ty: stream, defaulted: false, rest: false },
            Parameter { label: crate::symbol::Name::intern("second"), ty: stream, defaulted: true, rest: false },
        ], result: stream, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let reason = graph.reason(Span::at(SourceId::new(0), 0), None).unwrap();
        let requirement = graph.require_callable_invocation(InvocationCall { callable: signature,
            arguments: vec![InvocationArgument { kind: InvocationArgumentKind::PositionalSplice, ty: list }],
            result: stream, effects: EffectSummary::Closed(EffectSet::EMPTY), domain: CallableDomain::Pure }, reason).unwrap();
        graph.solve().unwrap();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let owner = declaration(0);
        let first = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: owner, index: 0 });
        let second = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: owner, index: 1 });
        let callee = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: owner, origin: None });
        let default = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::ENV, EffectSet::EMPTY)));
        let item = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::TIME, EffectSet::EMPTY)));
        let argument = push(&mut graph, &mut flows, ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![ProducerPathComponent::ListItem]), input: item }] });
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(20), callee, arguments: vec![argument] });
        let mut inputs = ProducerEvaluationInputs::default();
        inputs.declarations.insert(owner, ProducerEvaluationDeclaration { parameters: vec![first, second], result: Some(second), defaults: BTreeMap::from([(1, default)]), ..Default::default() });
        inputs.invocation_requirements.insert(call(20), requirement);
        let value = evaluate_producer_flow(&mut graph, &flows, root, &inputs).unwrap();
        assert!(!value.pending);
        assert_eq!(value.profile, known(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0), EffectSet::EMPTY));
        let named_requirement = graph.require_callable_invocation(InvocationCall { callable: signature,
            arguments: vec![InvocationArgument { kind: InvocationArgumentKind::PositionalSplice, ty: list },
                InvocationArgument { kind: InvocationArgumentKind::Named(crate::symbol::Name::intern("first")), ty: stream }],
            result: stream, effects: EffectSummary::Closed(EffectSet::EMPTY), domain: CallableDomain::Pure }, reason).unwrap();
        graph.solve().unwrap();
        let supplied = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::PROCESS, EffectSet::EMPTY)));
        let named_root = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(21), callee, arguments: vec![argument, supplied] });
        inputs.invocation_requirements.insert(call(21), named_requirement);
        let named = evaluate_producer_flow(&mut graph, &flows, named_root, &inputs).unwrap();
        assert!(!named.pending);
        assert_eq!(named.profile, known(EffectSet::ENV, EffectSet::EMPTY), "the checked definite default excludes any earlier speculative splice contribution");
    }

    #[test]
    fn long_alias_chains_use_a_charged_worklist_instead_of_the_host_stack() {
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let mut root = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::TIME, EffectSet::ENV)));
        for _ in 0..2_000 { root = push(&mut graph, &mut flows, ProducerFlowKind::Join { inputs: vec![root] }); }
        let before = graph.counters().work_units;
        let value = evaluate_producer_flow(&mut graph, &flows, root, &ProducerEvaluationInputs::default()).unwrap();
        assert_eq!(value.profile, known(EffectSet::TIME, EffectSet::ENV));
        assert!(graph.counters().work_units - before < 100_000);
    }

    #[test]
    fn multiple_alias_roots_share_the_dependency_worklist() {
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let mut root = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::TIME, EffectSet::ENV)));
        let mut roots = Vec::new();
        for _ in 0..2_000 { root = push(&mut graph, &mut flows, ProducerFlowKind::Join { inputs: vec![root] }); roots.push(root); }
        let before = graph.counters().work_units;
        let values = evaluate_producer_flows(&mut graph, &flows, &roots, &ProducerEvaluationInputs::default()).unwrap();
        assert_eq!(values.len(), roots.len());
        assert!(values.iter().all(|value| value.profile == known(EffectSet::TIME, EffectSet::ENV)));
        assert!(graph.counters().work_units - before < 100_000);
    }

    #[test]
    fn recursive_declaration_results_reach_a_shared_monotone_fixed_point() {
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let owner = declaration(0);
        let base = push(&mut graph, &mut flows, ProducerFlowKind::Known(known(EffectSet::TIME, EffectSet::ENV)));
        let callee = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: owner, origin: None });
        let recursive = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(20), callee, arguments: vec![] });
        let result = push(&mut graph, &mut flows, ProducerFlowKind::Join { inputs: vec![base, recursive] });
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(21), callee, arguments: vec![] });
        let mut inputs = ProducerEvaluationInputs::default();
        inputs.declarations.insert(owner, ProducerEvaluationDeclaration { result: Some(result), ..Default::default() });
        for index in [20, 21] { inputs.call_bindings.insert(call(index), binding(&[])); }
        let before = graph.counters().work_units;
        let value = evaluate_producer_flow(&mut graph, &flows, root, &inputs).unwrap();
        assert!(!value.pending);
        assert_eq!(value.profile, known(EffectSet::TIME, EffectSet::ENV));
        assert!(graph.counters().work_units - before < 1_000);
    }

    #[test]
    fn published_formal_effects_substitute_by_exact_binder_in_each_call() {
        use crate::sema::inference::{Atom, Generalization};
        let mut graph = InferenceContext::default();
        let item = graph.atom(Atom::Int).unwrap();
        let pull = graph.fresh_effect_at(1, None).unwrap();
        let close = graph.fresh_effect_at(1, None).unwrap();
        let original_pull = EffectSummary::Variable(pull);
        let original_close = EffectSummary::Variable(close);
        let scope = graph.generalize_with_effect_roots(item, 0, Generalization::Allowed, &[], &[original_pull, original_close]).unwrap();
        let formals = graph.scheme(scope).unwrap().effect_roots.clone();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let owner = declaration(0);
        let result = push(&mut graph, &mut flows, ProducerFlowKind::Known(BTreeMap::from([(ProducerPath::default(), ProducerEffects { pull: original_pull, close: original_close })])));
        let callee = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: owner, origin: None });
        let time = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(20), callee, arguments: vec![] });
        let env = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(21), callee, arguments: vec![] });
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![ProducerPathComponent::ResultSuccess]), input: time }, ProducerFlowField { path: ProducerPath(vec![ProducerPathComponent::ResultError]), input: env }] });
        let mut inputs = ProducerEvaluationInputs::default();
        inputs.declarations.insert(owner, ProducerEvaluationDeclaration { result: Some(result), ..Default::default() });
        for index in [20, 21] { inputs.call_bindings.insert(call(index), binding(&[])); }
        inputs.call_effect_substitutions.insert(call(20), vec![(formals[0], EffectSummary::Closed(EffectSet::TIME)), (formals[1], EffectSummary::Closed(EffectSet::EMPTY))]);
        inputs.call_effect_substitutions.insert(call(21), vec![(formals[0], EffectSummary::Closed(EffectSet::EMPTY)), (formals[1], EffectSummary::Closed(EffectSet::ENV))]);
        let value = evaluate_producer_flow(&mut graph, &flows, root, &inputs).unwrap();
        assert_eq!(value.profile[&ProducerPath(vec![ProducerPathComponent::ResultSuccess])], known(EffectSet::TIME, EffectSet::EMPTY)[&ProducerPath::default()]);
        assert_eq!(value.profile[&ProducerPath(vec![ProducerPathComponent::ResultError])], known(EffectSet::EMPTY, EffectSet::ENV)[&ProducerPath::default()]);
        assert_eq!(graph.scheme(scope).unwrap().effect_roots, formals);
    }

    #[test]
    fn symbolic_parameter_projection_preserves_formal_paths_through_a_call() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let caller = declaration(0);
        let callee_owner = declaration(1);
        let parameter = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: caller, index: 2 });
        let field = crate::symbol::Name::intern("nested");
        let argument = push(&mut graph, &mut flows, ProducerFlowKind::Project { input: parameter, path: ProducerPath(vec![ProducerPathComponent::RecordField(field)]) });
        let formal = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: callee_owner, index: 0 });
        let result = push(&mut graph, &mut flows, ProducerFlowKind::Project { input: formal, path: ProducerPath(vec![ProducerPathComponent::ListItem]) });
        let callee = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: callee_owner, origin: None });
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(20), callee, arguments: vec![argument] });
        let mut inputs = ProducerEvaluationInputs::default();
        inputs.declarations.insert(caller, ProducerEvaluationDeclaration::default());
        inputs.declarations.insert(callee_owner, ProducerEvaluationDeclaration { parameters: vec![formal], result: Some(result), ..Default::default() });
        inputs.call_bindings.insert(call(20), binding(&[0]));
        let value = evaluate_producer_flow(&mut graph, &flows, root, &inputs).unwrap();
        assert!(!value.pending);
        assert!(value.profile.is_empty());
        assert!(value.opaque_paths.is_empty());
        assert_eq!(value.symbolic_parameters, BTreeMap::from([(ProducerPath::default(), BTreeSet::from([ProducerEvaluationParameter { declaration: caller, index: 2, path: ProducerPath(vec![ProducerPathComponent::RecordField(field), ProducerPathComponent::ListItem]), excluded_paths: BTreeSet::new() }]))]));
        assert!(inputs.parameters.is_empty());
    }

    #[test]
    fn aggregate_parameter_paths_distinguish_output_position_from_formal_projection() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let owner = declaration(0);
        let parameter = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: owner, index: 0 });
        let field = crate::symbol::Name::intern("field");
        let aggregate = push(&mut graph, &mut flows, ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![ProducerPathComponent::RecordField(field)]), input: parameter }] });
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Project { input: aggregate, path: ProducerPath(vec![ProducerPathComponent::RecordField(field), ProducerPathComponent::ResultSuccess]) });
        let mut inputs = ProducerEvaluationInputs::default();
        inputs.declarations.insert(owner, ProducerEvaluationDeclaration::default());
        let value = evaluate_producer_flow(&mut graph, &flows, root, &inputs).unwrap();
        assert_eq!(value.symbolic_parameters, BTreeMap::from([(ProducerPath::default(), BTreeSet::from([ProducerEvaluationParameter { declaration: owner, index: 0, path: ProducerPath(vec![ProducerPathComponent::ResultSuccess]), excluded_paths: BTreeSet::new() }]))]));
    }

    #[test]
    fn operation_transfer_uses_the_call_instance_and_exact_input_projection() {
        use crate::sema::inference::{Arrow, Atom, CallableKind, CandidateTemplate, Generalization, OperationCall, Parameter, RowField, TypeNode};
        use super::super::{ProducerFlowOperationAlternative, ProducerFlowOperationTransfer};
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default();
        let span = Span::at(SourceId::new(0), 0);
        let why = graph.reason(span, None).unwrap();
        let item = graph.atom(Atom::Int).unwrap();
        let stream = graph.stream(item).unwrap();
        let list = graph.list(stream).unwrap();
        let field = crate::symbol::Name::intern("field");
        let row = graph.row(vec![RowField { label: field, ty: stream }], None).unwrap();
        let record = graph.record(row).unwrap();
        let param = |ty| Parameter { label: crate::symbol::Name::intern("value"), ty, defaulted: false, rest: false };
        let mut candidates = Vec::new();
        for (index, ty) in [list, record].into_iter().enumerate() {
            let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![param(ty)], result: stream, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
            let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
            let name = crate::symbol::Name::intern(format!("transfer{index}"));
            candidates.push(graph.register_candidate(CandidateTemplate { failure_projection: None, identity: name, public_label: name, scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![], effect_roles: vec![], output_effect_roles: vec![] }).unwrap());
        }
        let family = graph.register_family(&candidates).unwrap();
        let argument = graph.fresh(1, span).unwrap();
        let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(argument)], result: stream, effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: vec![], output_effect_bindings: vec![] }, why).unwrap();
        let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![param(argument)], result: stream, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[requirement]).unwrap();
        let mut instances = Vec::new();
        for ty in [list, record] {
            let instance = graph.instantiate(scheme, 0, why).unwrap();
            let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap() else { panic!() };
            graph.unify(signature.params[0].ty, ty, why).unwrap();
            graph.solve().unwrap(); instances.push(instance.requirements[0]);
        }
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let owner = declaration(0);
        let parameter = push(&mut graph, &mut flows, ProducerFlowKind::Parameter { declaration: owner, index: 0 });
        let paths = [ProducerPath(vec![ProducerPathComponent::ListItem]), ProducerPath(vec![ProducerPathComponent::RecordField(field)])];
        let alternatives = candidates.into_iter().zip(&paths).map(|(candidate, path)| ProducerFlowOperationAlternative { candidate, path: None, transfers: vec![ProducerFlowOperationTransfer { input: parameter, input_path: path.clone(), output_path: ProducerPath::default() }], opaque: false }).collect();
        let result = push(&mut graph, &mut flows, ProducerFlowKind::Operation { requirement, alternatives, outputs: ProducerEffects { pull: EffectSummary::Closed(EffectSet::EMPTY), close: EffectSummary::Closed(EffectSet::EMPTY) } });
        let callee = push(&mut graph, &mut flows, ProducerFlowKind::Callable { declaration: owner, origin: None });
        let mut roots = Vec::new();
        for (index, path) in paths.into_iter().enumerate() {
            let profile = known(if index == 0 { EffectSet::TIME } else { EffectSet::EMPTY }, if index == 1 { EffectSet::ENV } else { EffectSet::EMPTY });
            let leaf = push(&mut graph, &mut flows, ProducerFlowKind::Known(profile));
            let argument = push(&mut graph, &mut flows, ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path, input: leaf }] });
            roots.push(push(&mut graph, &mut flows, ProducerFlowKind::Apply { call: call(20 + index), callee, arguments: vec![argument] }));
        }
        let mut inputs = ProducerEvaluationInputs::default();
        inputs.declarations.insert(owner, ProducerEvaluationDeclaration { parameters: vec![parameter], result: Some(result), ..Default::default() });
        for (index, instance) in instances.into_iter().enumerate() { inputs.call_bindings.insert(call(20 + index), binding(&[0])); inputs.call_requirement_origins.insert(call(20 + index), vec![(requirement, instance)]); }
        assert!(evaluate_producer_flow(&mut graph, &flows, result, &inputs).unwrap().pending);
        let values = evaluate_producer_flows(&mut graph, &flows, &roots, &inputs).unwrap();
        assert!(values.iter().all(|value| !value.pending));
        assert_eq!(values[0].profile, known(EffectSet::TIME, EffectSet::EMPTY));
        assert_eq!(values[1].profile, known(EffectSet::EMPTY, EffectSet::ENV));
    }

    #[test]
    fn flow_evaluation_rejects_foreign_owners_and_charges_its_work() {
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Empty);
        assert!(matches!(evaluate_producer_flow(&mut InferenceContext::default(), &flows, root, &ProducerEvaluationInputs::default()), Err(InferenceError::ForeignHandle)));
        let mut graph = InferenceContext::new(Limits { work_units: 2, ..Limits::default() });
        let mut flows = ProducerFlowGraph::new(graph.owner());
        let root = push(&mut graph, &mut flows, ProducerFlowKind::Empty);
        assert!(matches!(evaluate_producer_flow(&mut graph, &flows, root, &ProducerEvaluationInputs::default()), Err(InferenceError::Limit(_))));
    }
}
