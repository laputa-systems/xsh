use super::{BindingIdentity, DeclarationIdentity, ExpressionIdentity, StatementIdentity, ProducerPath, ProducerProfile};
use crate::sema::inference::{GraphOwner, InferenceContext, InferenceError, CandidateId, RequirementId, NativeAuthority};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProducerFlowId {
    owner: GraphOwner,
    index: u32,
}

impl ProducerFlowId {
    pub fn owner(self) -> GraphOwner { self.owner }
    pub fn index(self) -> usize { self.index as usize }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProducerFlowSource {
    Expression(ExpressionIdentity),
    Statement(StatementIdentity),
    Stage(super::StageIdentity),
    Comprehension(super::ComprehensionIdentity),
    Binding { identity: BindingIdentity, version: u32 },
    Parameter { declaration: DeclarationIdentity, index: u32 },
    DeclarationResult(DeclarationIdentity),
}

#[derive(Clone, Debug)]
pub struct ProducerFlowField {
    pub path: ProducerPath,
    pub input: ProducerFlowId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProducerFlowOperationTransfer {
    pub input: ProducerFlowId,
    pub input_path: ProducerPath,
    pub output_path: ProducerPath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProducerFlowOperationAlternative {
    pub candidate: CandidateId,
    pub path: Option<ProducerPath>,
    pub transfers: Vec<ProducerFlowOperationTransfer>,
    pub opaque: bool,
}

/// These relationships retain source values independently of type equality.
/// Two parameters can have the same type and carry different producer handles.
#[derive(Clone, Debug)]
pub enum ProducerFlowKind {
    Empty,
    Opaque,
    Known(ProducerProfile),
    Operation { requirement: RequirementId, alternatives: Vec<ProducerFlowOperationAlternative>, outputs: super::ProducerEffects },
    Addition { requirement: RequirementId, left: ProducerFlowId, right: ProducerFlowId },
    Parameter { declaration: DeclarationIdentity, index: u32 },
    CapturedBinding { identity: BindingIdentity, version: u32, input: ProducerFlowId },
    Project { input: ProducerFlowId, path: ProducerPath },
    /// Nullable completion preserves one payload layer, including inputs
    /// already carried in an Optional value.
    OptionalLift { input: ProducerFlowId },
    Aggregate { entries: Vec<ProducerFlowField> },
    RecordUpdate { base: ProducerFlowId, replacements: Vec<ProducerFlowField> },
    Join { inputs: Vec<ProducerFlowId> },
    Callable { declaration: DeclarationIdentity, origin: Option<crate::sema::inference::TypeId> },
    NativeCallable { authority: NativeAuthority },
    Apply { call: ExpressionIdentity, callee: ProducerFlowId, arguments: Vec<ProducerFlowId> },
    StageApply { stage: super::StageIdentity, callee: ProducerFlowId, arguments: Vec<ProducerFlowId> },
}

impl ProducerFlowKind {
    fn inputs(&self) -> Vec<ProducerFlowId> {
        match self {
            Self::CapturedBinding { input, .. } | Self::Project { input, .. } | Self::OptionalLift { input } => vec![*input],
            Self::Aggregate { entries } => entries.iter().map(|entry| entry.input).collect(),
            Self::RecordUpdate { base, replacements } => std::iter::once(*base).chain(replacements.iter().map(|entry| entry.input)).collect(),
            Self::Join { inputs } => inputs.clone(),
            Self::Apply { callee, arguments, .. } | Self::StageApply { callee, arguments, .. } => std::iter::once(*callee).chain(arguments.iter().copied()).collect(),
            Self::Operation { alternatives, .. } => alternatives.iter().flat_map(|alternative| alternative.transfers.iter().map(|transfer| transfer.input)).collect(),
            Self::Addition { left, right, .. } => vec![*left, *right],
            Self::Empty | Self::Opaque | Self::Known(_) | Self::Parameter { .. } | Self::Callable { .. } | Self::NativeCallable { .. } => Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProducerFlowNode {
    pub source: ProducerFlowSource,
    pub kind: ProducerFlowKind,
}

/// Direct value edges point to existing nodes. Calls can refer to recursive
/// declaration results, whose semantic closure requires a bounded fixed point.
#[derive(Debug)]
pub struct ProducerFlowGraph {
    owner: GraphOwner,
    nodes: Vec<ProducerFlowNode>,
}

impl ProducerFlowGraph {
    pub(super) fn new(owner: GraphOwner) -> Self { Self { owner, nodes: Vec::new() } }
    pub fn owner(&self) -> GraphOwner { self.owner }
    pub fn nodes(&self) -> impl ExactSizeIterator<Item = &ProducerFlowNode> { self.nodes.iter() }
    pub fn node(&self, id: ProducerFlowId) -> Result<&ProducerFlowNode, InferenceError> {
        if id.owner != self.owner { return Err(InferenceError::ForeignHandle); }
        self.nodes.get(id.index()).ok_or(InferenceError::ForeignHandle)
    }

    pub(super) fn push(&mut self, graph: &mut InferenceContext, source: ProducerFlowSource, kind: ProducerFlowKind) -> Result<ProducerFlowId, InferenceError> {
        if graph.owner() != self.owner { return Err(InferenceError::ForeignHandle); }
        if let ProducerFlowKind::Operation { requirement, alternatives, .. } = &kind {
            let crate::sema::inference::RequirementTemplate::Operation { family, .. } = graph.requirement_template(*requirement)? else { return Err(InferenceError::InvalidScheme); };
            let mut candidates = graph.family(family)?.to_vec();
            let mut actual: Vec<_> = alternatives.iter().map(|alternative| alternative.candidate).collect();
            let work = actual.len().saturating_mul(actual.len().max(1).ilog2() as usize + 2);
            graph.charge_source_fact_work(work as u64)?;
            actual.sort_unstable();
            candidates.sort_unstable();
            if actual != candidates { return Err(InferenceError::InvalidScheme); }
        }
        if let ProducerFlowKind::Addition { requirement, .. } = &kind {
            if !matches!(graph.requirement_template(*requirement)?, crate::sema::inference::RequirementTemplate::Add { .. }) { return Err(InferenceError::InvalidScheme); }
        }
        if let ProducerFlowKind::NativeCallable { authority } = &kind { graph.native_authority_signature(*authority)?; }
        if let ProducerFlowKind::Callable { origin: Some(origin), .. } = &kind {
            if !matches!(graph.node(graph.resolved(*origin)?)?, crate::sema::inference::TypeNode::Arrow(_)) { return Err(InferenceError::InvalidScheme); }
        }
        let inputs = kind.inputs();
        graph.charge_source_fact_work(inputs.len() as u64 + 1)?;
        for input in &inputs { self.node(*input)?; }
        let index = u32::try_from(self.nodes.len()).map_err(|_| InferenceError::Limit("producer flow nodes"))?;
        graph.charge_source_fact_nodes(1)?;
        graph.charge_source_fact_edges(inputs.len() as u64 + u64::from(matches!(kind, ProducerFlowKind::Callable { origin: Some(_), .. })))?;
        self.nodes.push(ProducerFlowNode { source, kind });
        Ok(ProducerFlowId { owner: self.owner, index })
    }

    #[cfg(test)]
    pub(crate) fn push_fixture(&mut self, graph: &mut InferenceContext, source: ProducerFlowSource, kind: ProducerFlowKind) -> Result<ProducerFlowId, InferenceError> {
        self.push(graph, source, kind)
    }

    pub(super) fn normalize_effects(&mut self, graph: &crate::sema::inference::SolvedGraph) -> Result<(), InferenceError> {
        for node in &mut self.nodes {
            if let ProducerFlowKind::Operation { outputs, .. } = &mut node.kind {
                outputs.pull = graph.closed_effect_summary(outputs.pull)?;
                outputs.close = graph.closed_effect_summary(outputs.close)?;
            }
            if let ProducerFlowKind::Known(profile) = &mut node.kind {
                for effects in profile.values_mut() {
                    effects.pull = graph.closed_effect_summary(effects.pull)?;
                    effects.close = graph.closed_effect_summary(effects.close)?;
                }
            }
        }
        Ok(())
    }

    pub fn retained_bytes(&self) -> usize {
        let profile_bytes = |profile: &ProducerProfile| profile.len() * std::mem::size_of::<(ProducerPath, super::ProducerEffects)>()
            + profile.keys().map(|path| path.0.capacity() * std::mem::size_of::<super::ProducerPathComponent>()).sum::<usize>();
        self.nodes.capacity() * std::mem::size_of::<ProducerFlowNode>() + self.nodes.iter().map(|node| match &node.kind {
            ProducerFlowKind::Known(profile) => profile_bytes(profile),
            ProducerFlowKind::Operation { alternatives, .. } => alternatives.capacity() * std::mem::size_of::<ProducerFlowOperationAlternative>()
                + alternatives.iter().filter_map(|alternative| alternative.path.as_ref()).map(|path| path.0.capacity() * std::mem::size_of::<super::ProducerPathComponent>()).sum::<usize>()
                + alternatives.iter().map(|alternative| alternative.transfers.capacity() * std::mem::size_of::<ProducerFlowOperationTransfer>() + alternative.transfers.iter().map(|transfer| (transfer.input_path.0.capacity() + transfer.output_path.0.capacity()) * std::mem::size_of::<super::ProducerPathComponent>()).sum::<usize>()).sum::<usize>(),
            ProducerFlowKind::Project { path, .. } => path.0.capacity() * std::mem::size_of::<super::ProducerPathComponent>(),
            ProducerFlowKind::Aggregate { entries } => entries.capacity() * std::mem::size_of::<ProducerFlowField>()
                + entries.iter().map(|entry| entry.path.0.capacity() * std::mem::size_of::<super::ProducerPathComponent>()).sum::<usize>(),
            ProducerFlowKind::RecordUpdate { replacements, .. } => replacements.capacity() * std::mem::size_of::<ProducerFlowField>()
                + replacements.iter().map(|entry| entry.path.0.capacity() * std::mem::size_of::<super::ProducerPathComponent>()).sum::<usize>(),
            ProducerFlowKind::Join { inputs } => inputs.capacity() * std::mem::size_of::<ProducerFlowId>(),
            ProducerFlowKind::Apply { arguments, .. } | ProducerFlowKind::StageApply { arguments, .. } => arguments.capacity() * std::mem::size_of::<ProducerFlowId>(),
            _ => 0,
        }).sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::inference::Limits;
    use crate::source::SourceId;
    use crate::syntax::arena::ExprId;

    fn source() -> ProducerFlowSource {
        ProducerFlowSource::Expression(ExpressionIdentity { source: SourceId::new(0), namespace: None, expression: ExprId::from_index(0) })
    }

    #[test]
    fn producer_flow_edges_reject_foreign_graph_nodes() {
        let mut original = InferenceContext::default();
        let mut original_flows = ProducerFlowGraph::new(original.owner());
        let foreign = original_flows.push(&mut original, source(), ProducerFlowKind::Empty).unwrap();
        let mut graph = InferenceContext::default();
        let mut flows = ProducerFlowGraph::new(graph.owner());
        assert!(matches!(flows.push(&mut graph, source(), ProducerFlowKind::Join { inputs: vec![foreign] }), Err(InferenceError::ForeignHandle)));
        assert_eq!(flows.nodes().len(), 0);
        assert!(matches!(flows.node(foreign), Err(InferenceError::ForeignHandle)));
    }

    #[test]
    fn producer_flow_nodes_share_the_type_graph_resource_limit() {
        let mut graph = InferenceContext::new(Limits { type_row_nodes: 1, ..Limits::default() });
        let mut flows = ProducerFlowGraph::new(graph.owner());
        flows.push(&mut graph, source(), ProducerFlowKind::Empty).unwrap();
        assert!(matches!(flows.push(&mut graph, source(), ProducerFlowKind::Empty), Err(InferenceError::Limit(_))));
        assert_eq!(flows.nodes().len(), 1);
        assert_eq!(graph.counters().attempted_nodes, 2);
    }
}

impl super::Checker {
    pub(super) fn push_source_producer_flow(&mut self, source: ProducerFlowSource, kind: ProducerFlowKind, span: crate::source::Span) -> Option<ProducerFlowId> {
        let result = {
            let mut state = self.generic.borrow_mut();
            let facts = &mut state.facts;
            facts.producer_flows.push(&mut facts.graph, source, kind)
        };
        match result {
            Ok(flow) => { self.generic.borrow_mut().producer_inputs.origins.insert(flow, span); Some(flow) }
            Err(error) => { self.graph_error(span, error); None }
        }
    }

    pub(super) fn evaluate_source_producer_flow(&mut self, flow: ProducerFlowId, span: crate::source::Span) -> Option<super::producer_eval::ProducerEvaluationValue> {
        let outcome = {
            let mut state = self.generic.borrow_mut();
            let state = &mut *state;
            state.producer_inputs.level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
            super::producer_eval::evaluate_producer_flow_with_registry(&mut state.facts.graph, &state.facts.producer_flows, flow, &state.producer_inputs, &state.registry)
        };
        match outcome { Ok(value) => Some(value), Err(error) => { self.graph_error(span, error); None } }
    }

    pub(super) fn producer_effects_for_flow(&mut self, flow: ProducerFlowId, path: &ProducerPath, span: crate::source::Span) -> Option<super::ProducerEffects> {
        use crate::sema::inference::{EffectSummary, EffectSet};
        let value = self.evaluate_source_producer_flow(flow, span)?;
        let demands: Vec<_> = value.symbolic_parameters.iter().filter(|(output, _)| path.0.starts_with(&output.0)).flat_map(|(output, parameters)| {
            parameters.iter().cloned().filter_map(|mut parameter| {
                parameter.path.0.extend_from_slice(&path.0[output.0.len()..]);
                (!parameter.excluded_paths.iter().any(|excluded| parameter.path.0.starts_with(&excluded.0))).then_some(parameter)
            })
        }).collect();
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            for parameter in demands {
                state.facts.graph.charge_source_fact_work(parameter.path.0.len() as u64 + 1)?;
                let profile = state.pending.get(&parameter.declaration).and_then(|pending| pending.parameter_producers.get(parameter.index as usize)).ok_or(InferenceError::InvalidScheme)?;
                if profile.contains_key(&parameter.path) { continue; }
                if state.completed.contains(&parameter.declaration) { return Err(InferenceError::Boundary("producer consumption cannot add ports to a completed declaration")); }
                let effects = super::ProducerEffects {
                    pull: EffectSummary::Variable(state.facts.graph.fresh_effect_at(1, None)?),
                    close: EffectSummary::Variable(state.facts.graph.fresh_effect_at(1, None)?),
                };
                state.facts.graph.charge_source_fact_edges(2)?;
                state.pending.get_mut(&parameter.declaration).unwrap().parameter_producers[parameter.index as usize].insert(parameter.path.clone(), effects);
                state.producer_inputs.parameters.entry((parameter.declaration, parameter.index)).or_default().profile.insert(parameter.path, effects);
            }
            Ok::<_, InferenceError>(())
        })();
        if let Err(error) = outcome { self.graph_error(span, error); return None; }
        let value = self.evaluate_source_producer_flow(flow, span)?;
        if value.pending_at(path) { self.graph_error(span, InferenceError::Boundary("producer permissions depend on an unresolved source value")); return None; }
        if value.opaque_at(path) {
            return Some(super::ProducerEffects { pull: EffectSummary::Unknown, close: EffectSummary::Unknown });
        }
        if !value.profile.is_empty() && !value.pending && value.pending_paths.is_empty() && value.opaque_paths.is_empty() {
            let retained = (|| {
                let mut state = self.generic.borrow_mut();
                if let ProducerFlowSource::Expression(identity) = state.facts.producer_flows.node(flow)?.source {
                    state.facts.graph.charge_source_fact_work(value.profile.keys().map(|path| path.0.len() as u64 + 1).sum())?;
                    state.facts.expression_producers.insert(identity, value.profile.clone());
                }
                Ok::<_, InferenceError>(())
            })();
            if let Err(error) = retained { self.graph_error(span, error); return None; }
        }
        Some(value.profile.get(path).copied().unwrap_or(super::ProducerEffects { pull: EffectSummary::Closed(EffectSet::EMPTY), close: EffectSummary::Closed(EffectSet::EMPTY) }))
    }

    pub(super) fn producer_effects_for_expression(&mut self, arena: &crate::syntax::arena::ArenaProgram, expression: crate::syntax::arena::ExprId, path: &ProducerPath, span: crate::source::Span) -> Option<super::ProducerEffects> {
        let flow = self.generic.borrow().facts.expression_producer_flows.get(&self.expression_identity(arena, expression)).copied();
        let Some(flow) = flow else { self.graph_error(span, InferenceError::Boundary("producer value has no checked source ownership")); return None; };
        self.producer_effects_for_flow(flow, path, span)
    }

    pub(super) fn argument_producer_flows(&mut self, arena: &crate::syntax::arena::ArenaProgram, call: ExpressionIdentity, _span: crate::source::Span) -> Result<Vec<ProducerFlowId>, InferenceError> {
        use crate::sema::arguments::ArgumentValueSource as Source;
        use super::ProducerPathComponent as Path;
        let sources = self.generic.borrow().facts.argument_sources.get(&call).cloned().ok_or(InferenceError::Boundary("call has no source argument binding"))?;
        let mut arguments = Vec::with_capacity(sources.len());
        for source in sources {
            let (expression, path) = match source.value {
                Source::Expression(expression) => (expression, None),
                Source::RecordField { record, field } => (record, Some(ProducerPath(vec![Path::RecordField(field)]))),
                Source::PositionalSplice(expression) => (expression, None),
            };
            let input = self.generic.borrow().facts.expression_producer_flows.get(&self.expression_identity(arena, expression)).copied().ok_or(InferenceError::Boundary("call argument has no checked value flow"))?;
            let flow = if let Some(path) = path {
                let source = ProducerFlowSource::Expression(self.expression_identity(arena, expression));
                self.push_source_producer_flow(source, ProducerFlowKind::Project { input, path }, arena.arena.expr(expression).span).ok_or(InferenceError::Boundary("call argument projection cannot be retained"))?
            } else { input };
            arguments.push(flow);
        }
        self.generic.borrow_mut().facts.graph.charge_source_fact_work(arguments.len() as u64 + 1)?;
        Ok(arguments)
    }

    pub(super) fn bind_call_producer_arguments(&mut self, arena: &crate::syntax::arena::ArenaProgram, call: ExpressionIdentity, declaration: DeclarationIdentity, binding: &super::CallBinding, profiles: &[ProducerProfile], effect_substitutions: &[crate::sema::inference::EffectSummary], span: crate::source::Span) -> Result<Vec<ProducerProfile>, InferenceError> {
        use crate::sema::inference::{InvocationArgumentKind, RequirementTemplate};
        use super::ProducerPathComponent as Path;
        if profiles.iter().all(ProducerProfile::is_empty) {
            let count = self.generic.borrow().facts.argument_sources.get(&call).ok_or(InferenceError::Boundary("call has no source argument binding"))?.len();
            return Ok(vec![ProducerProfile::new(); count]);
        }
        let flows = self.argument_producer_flows(arena, call, span)?;
        let (kinds, defaults, scheme) = {
            let state = self.generic.borrow();
            let kinds = if let Some(invocation) = state.facts.invocations.get(&call) {
                let RequirementTemplate::CallableInvocation { call } = state.facts.graph.requirement_template(invocation.requirement)? else { return Err(InferenceError::InvalidScheme); };
                state.facts.graph.invocation_call(call)?.arguments.iter().map(|argument| argument.kind).collect::<Vec<_>>()
            } else { vec![InvocationArgumentKind::Positional; flows.len()] };
            let defaults = state.producer_inputs.declarations.get(&declaration).map(|declaration| declaration.defaults.clone()).unwrap_or_default();
            (kinds, defaults, state.facts.declarations.get(&declaration).map(|declaration| declaration.scheme))
        };
        let transfers = super::producer_eval::producer_argument_transfers(&mut self.generic.borrow_mut().facts.graph, binding, &kinds)?;
        let mut formal_flows: std::collections::BTreeMap<usize, Vec<ProducerFlowId>> = std::collections::BTreeMap::new();
        for transfer in transfers {
            let mut input = *flows.get(transfer.argument).ok_or(InferenceError::InvalidScheme)?;
            if transfer.splice {
                input = self.push_source_producer_flow(ProducerFlowSource::Expression(call), ProducerFlowKind::Project { input, path: ProducerPath(vec![Path::ListItem]) }, span).ok_or(InferenceError::InvalidScheme)?;
            }
            if transfer.rest {
                input = self.push_source_producer_flow(ProducerFlowSource::Expression(call), ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![Path::ListItem]), input }] }, span).ok_or(InferenceError::InvalidScheme)?;
            }
            formal_flows.entry(transfer.slot).or_default().push(input);
        }
        let default_slots = binding.default_slots.iter().copied().chain(binding.dynamic.iter().flat_map(|dynamic| dynamic.conditional_default_slots.iter().copied()));
        for slot in default_slots {
            if profiles.get(slot).is_none_or(ProducerProfile::is_empty) { continue; }
            let input = *defaults.get(&(slot as u32)).ok_or(InferenceError::Boundary("producer default lacks its checked source flow"))?;
            let value = self.evaluate_source_producer_flow(input, span).ok_or(InferenceError::InvalidScheme)?;
            if value.pending || !value.pending_paths.is_empty() || !value.opaque_paths.is_empty() { return Err(InferenceError::Boundary("producer default permissions cannot be established")); }
            let profile = if let Some(scheme) = scheme {
                instantiate_producer_profile(&mut self.generic.borrow_mut().facts.graph, &value.profile, scheme, effect_substitutions)?
            } else { value.profile };
            let input = self.push_source_producer_flow(ProducerFlowSource::Expression(call), ProducerFlowKind::Known(profile), span).ok_or(InferenceError::InvalidScheme)?;
            formal_flows.entry(slot).or_default().push(input);
        }
        for (slot, inputs) in formal_flows {
            let expected = profiles.get(slot).ok_or(InferenceError::InvalidScheme)?;
            if expected.is_empty() { continue; }
            let flow = self.push_source_producer_flow(ProducerFlowSource::Expression(call), ProducerFlowKind::Join { inputs }, span).ok_or(InferenceError::InvalidScheme)?;
            for (path, expected) in expected {
                let actual = self.producer_effects_for_flow(flow, path, span).ok_or(InferenceError::Boundary("call argument producer permissions cannot be established"))?;
                let mut state = self.generic.borrow_mut();
                let reason = state.facts.graph.reason(span, None)?;
                state.facts.graph.equate_effects(actual.pull, expected.pull, reason)?;
                state.facts.graph.equate_effects(actual.close, expected.close, reason)?;
            }
        }
        flows.into_iter().map(|flow| self.evaluate_source_producer_flow(flow, span).map(|value| value.profile).ok_or(InferenceError::InvalidScheme)).collect()
    }

    pub(super) fn registry_operation_producer_flow_from_flows(&mut self, requirement: RequirementId, receiver: Option<ProducerFlowId>, arguments: &[Option<ProducerFlowId>], source: ProducerFlowSource, span: crate::source::Span) -> Result<ProducerFlowId, InferenceError> {
        use crate::sema::registry_graph::{RegistryLifecycle, RegistryProducerTransferPlan, RegistryProducerInput, RegistryProducerComponent as Component};
        use crate::sema::inference::{EffectSummary, EffectSet, ProducerRole, RequirementTemplate};
        let path = |components: &[Component]| ProducerPath(components.iter().map(|component| match component {
            Component::ListItem => super::ProducerPathComponent::ListItem,
            Component::MapKey => super::ProducerPathComponent::MapKey,
            Component::MapValue => super::ProducerPathComponent::MapValue,
            Component::OptionalPayload => super::ProducerPathComponent::OptionalPayload,
            Component::ResultSuccess => super::ProducerPathComponent::ResultSuccess,
            Component::ResultError => super::ProducerPathComponent::ResultError,
            Component::RecordField(name) => super::ProducerPathComponent::RecordField(*name),
        }).collect());
        let (alternatives, outputs) = {
            let mut state = self.generic.borrow_mut();
            let RequirementTemplate::Operation { family, call } = state.facts.graph.requirement_template(requirement)? else { return Err(InferenceError::InvalidScheme); };
            let candidates = state.facts.graph.family(family)?.to_vec();
            let bindings = state.facts.graph.operation_call(call)?.output_effect_bindings.clone();
            let mut alternatives = Vec::with_capacity(candidates.len());
            let mut has_native = false;
            for candidate in candidates {
                let metadata = state.registry.metadata(&state.facts.graph, candidate)?;
                let native = matches!(metadata.lifecycle, RegistryLifecycle::ResultProducer { .. });
                has_native |= native;
                let plan = state.registry.producer_result_transfer(&state.facts.graph, candidate)?.clone();
                let mut alternative = ProducerFlowOperationAlternative { candidate, path: native.then(|| ProducerPath(vec![super::ProducerPathComponent::ResultSuccess])), transfers: Vec::new(), opaque: false };
                match plan {
                    RegistryProducerTransferPlan::Empty => {},
                    RegistryProducerTransferPlan::Opaque => alternative.opaque = !native,
                    RegistryProducerTransferPlan::Transfers(transfers) => for transfer in transfers.iter() {
                        state.facts.graph.charge_source_fact_work((transfer.input_path.len() + transfer.output_path.len() + 1) as u64)?;
                        let input = match transfer.input { RegistryProducerInput::Receiver => receiver, RegistryProducerInput::Argument(index) => arguments.get(index as usize).copied().flatten() };
                        if let Some(input) = input {
                            alternative.transfers.push(ProducerFlowOperationTransfer { input, input_path: path(&transfer.input_path), output_path: path(&transfer.output_path) });
                        } else { alternative.opaque = true; }
                    },
                }
                alternatives.push(alternative);
            }
            let outputs = if has_native {
                super::ProducerEffects {
                    pull: bindings.iter().find(|(role, _)| *role == ProducerRole::Pull).map(|(_, effect)| *effect).ok_or(InferenceError::InvalidScheme)?,
                    close: bindings.iter().find(|(role, _)| *role == ProducerRole::Close).map(|(_, effect)| *effect).ok_or(InferenceError::InvalidScheme)?,
                }
            } else { super::ProducerEffects { pull: EffectSummary::Closed(EffectSet::EMPTY), close: EffectSummary::Closed(EffectSet::EMPTY) } };
            (alternatives, outputs)
        };
        self.push_source_producer_flow(source, ProducerFlowKind::Operation { requirement, alternatives, outputs }, span).ok_or(InferenceError::Boundary("registry producer relationships cannot be retained"))
    }

    pub(super) fn record_registry_operation_producer_flow(&mut self, arena: &crate::syntax::arena::ArenaProgram, expression: crate::syntax::arena::ExprId, requirement: RequirementId, receiver: Option<crate::syntax::arena::ExprId>, arguments: &[crate::sema::arguments::ExpandedArgument], binding: &crate::sema::arguments::StaticArgumentBinding, span: crate::source::Span) -> Result<ProducerFlowId, InferenceError> {
        self.record_registry_operation_producer_flow_with_receiver_path(arena, expression, requirement, receiver, &ProducerPath::default(), arguments, binding, span)
    }

    pub(super) fn record_registry_operation_producer_flow_with_receiver_path(&mut self, arena: &crate::syntax::arena::ArenaProgram, expression: crate::syntax::arena::ExprId, requirement: RequirementId, receiver: Option<crate::syntax::arena::ExprId>, receiver_path: &ProducerPath, arguments: &[crate::sema::arguments::ExpandedArgument], binding: &crate::sema::arguments::StaticArgumentBinding, span: crate::source::Span) -> Result<ProducerFlowId, InferenceError> {
        let identity = self.expression_identity(arena, expression);
        self.record_argument_sources(identity, arguments)?;
        let mut formal = vec![None; binding.argument_slots.len() + binding.omitted_slots.len()];
        for (argument, &slot) in arguments.iter().zip(&binding.argument_slots) {
            use crate::sema::arguments::ArgumentValueSource as Source;
            let (expression, projection) = match argument.value {
                Source::Expression(expression) => (expression, None),
                Source::RecordField { record, field } => (record, Some(ProducerPath(vec![super::ProducerPathComponent::RecordField(field)]))),
                Source::PositionalSplice(expression) => (expression, Some(ProducerPath(vec![super::ProducerPathComponent::ListItem]))),
            };
            let input = self.generic.borrow().facts.expression_producer_flows.get(&self.expression_identity(arena, expression)).copied();
            formal[slot] = if let (Some(input), Some(path)) = (input, projection) { self.push_source_producer_flow(ProducerFlowSource::Expression(identity), ProducerFlowKind::Project { input, path }, argument.span) } else { input };
        }
        let receiver = receiver.and_then(|expression| self.generic.borrow().facts.expression_producer_flows.get(&self.expression_identity(arena, expression)).copied());
        let receiver = if let Some(input) = receiver && !receiver_path.0.is_empty() {
            Some(self.push_source_producer_flow(ProducerFlowSource::Expression(identity), ProducerFlowKind::Project { input, path: receiver_path.clone() }, span).ok_or(InferenceError::Boundary("checked receiver projection cannot be retained"))?)
        } else { receiver };
        let flow = self.registry_operation_producer_flow_from_flows(requirement, receiver, &formal, ProducerFlowSource::Expression(identity), span)?;
        self.generic.borrow_mut().facts.expression_producer_flows.insert(identity, flow);
        Ok(flow)
    }

    fn language_fallback_producer_flow_kind(&mut self, requirement: RequirementId, left: ProducerFlowId, right: ProducerFlowId) -> Result<ProducerFlowKind, InferenceError> {
        use crate::sema::inference::{EffectSet, EffectSummary, RequirementTemplate};
        use crate::sema::operation_graph::PreparedLanguageOperation;
        let mut state = self.generic.borrow_mut();
        let RequirementTemplate::Operation { family, .. } = state.facts.graph.requirement_template(requirement)? else { return Err(InferenceError::InvalidScheme); };
        let candidates = state.facts.graph.family(family)?.to_vec();
        state.facts.graph.charge_source_fact_work(candidates.len() as u64 * 5)?;
        let mut alternatives = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let PreparedLanguageOperation::Fallback { result } = state.language_operations.metadata(&state.facts.graph, candidate)?.operation else { return Err(InferenceError::InvalidScheme); };
            let payload = if result { super::ProducerPathComponent::ResultSuccess } else { super::ProducerPathComponent::OptionalPayload };
            alternatives.push(ProducerFlowOperationAlternative { candidate, path: None, opaque: false, transfers: vec![
                ProducerFlowOperationTransfer { input: left, input_path: ProducerPath(vec![payload]), output_path: ProducerPath::default() },
                ProducerFlowOperationTransfer { input: right, input_path: ProducerPath::default(), output_path: ProducerPath::default() },
            ] });
        }
        Ok(ProducerFlowKind::Operation { requirement, alternatives, outputs: super::ProducerEffects { pull: EffectSummary::Closed(EffectSet::EMPTY), close: EffectSummary::Closed(EffectSet::EMPTY) } })
    }

    pub(super) fn record_pattern_producer_flow(&mut self, arena: &crate::syntax::arena::ArenaProgram, pattern: crate::syntax::arena::PatternId, subject: crate::syntax::arena::ExprId) {
        use crate::syntax::arena::ArenaPatternKind as Pattern;
        use super::ProducerPathComponent as Path;
        if !self.graph_generation { return; }
        let identity = self.expression_identity(arena, subject);
        let Some(input) = self.generic.borrow().facts.expression_producer_flows.get(&identity).copied() else { return; };
        let span = arena.arena.expr(subject).span;
        let mut pending = vec![(pattern, ProducerPath::default())];
        let mut paths = std::collections::BTreeMap::<crate::symbol::Name, Vec<ProducerPath>>::new();
        while let Some((pattern, path)) = pending.pop() {
            let charged = {
                let mut state = self.generic.borrow_mut();
                if path.0.len() > state.facts.graph.limits().structural_depth { Err(InferenceError::Limit("pattern producer path depth")) }
                else { state.facts.graph.charge_source_fact_work(path.0.len() as u64 + 1) }
            };
            if let Err(error) = charged { self.graph_error(span, error); return; }
            match &arena.arena.pattern(pattern).kind {
                Pattern::Binding(name) if !self.tag_variants.contains_key(name) => { paths.entry(*name).or_default().push(path); }
                Pattern::Type { binding: Some(name), .. } => { paths.entry(*name).or_default().push(path); }
                Pattern::Alias { pattern, name, .. } => {
                    paths.entry(*name).or_default().push(path.clone());
                    pending.push((*pattern, path));
                }
                Pattern::Group(pattern) => pending.push((*pattern, path)),
                Pattern::Alternation(patterns) => {
                    for pattern in arena.arena.pattern_ids(*patterns) { pending.push((pattern, path.clone())); }
                }
                Pattern::Constructor { name, arg: Some(pattern) } if (*name == "Ok" || *name == "Err") && !self.tag_variants.contains_key(name) => {
                    let mut path = path;
                    path.0.push(if *name == "Ok" { Path::ResultSuccess } else { Path::ResultError });
                    pending.push((*pattern, path));
                }
                Pattern::Record { fields, .. } => {
                    for field in arena.arena.pattern_fields(*fields) {
                        let mut path = path.clone();
                        path.0.push(Path::RecordField(field.name));
                        pending.push((field.pattern, path));
                    }
                }
                Pattern::List { elements, rest } => {
                    for pattern in arena.arena.pattern_ids(*elements) {
                        let mut path = path.clone();
                        path.0.push(Path::ListItem);
                        pending.push((pattern, path));
                    }
                    if let Some(rest) = rest { pending.push((*rest, path)); }
                }
                _ => {}
            }
        }
        for (name, paths) in paths {
            if !self.current_scope().contains_key(&name) { continue; }
            let mut inputs = Vec::new();
            for path in paths {
                let Some(flow) = self.push_source_producer_flow(ProducerFlowSource::Expression(identity), ProducerFlowKind::Project { input, path }, span) else { return; };
                inputs.push(flow);
            }
            let Some(flow) = self.push_source_producer_flow(ProducerFlowSource::Expression(identity), ProducerFlowKind::Join { inputs }, span) else { return; };
            if let Some(binding) = self.current_scope_mut().get_mut(&name) { binding.producer_flow = Some(flow); }
        }
    }

    pub(super) fn record_parameter_default_producer_flow(&mut self, arena: &crate::syntax::arena::ArenaProgram, declaration: Option<DeclarationIdentity>, index: usize, expression: crate::syntax::arena::ExprId) {
        if !self.graph_generation { return; }
        let Some(declaration) = declaration else { return; };
        let mut state = self.generic.borrow_mut();
        let Some(flow) = state.facts.expression_producer_flows.get(&self.expression_identity(arena, expression)).copied() else { return; };
        if let Some(inputs) = state.producer_inputs.declarations.get_mut(&declaration) { inputs.defaults.insert(index as u32, flow); }
    }

    pub(super) fn record_expression_producer_flow(&mut self, arena: &crate::syntax::arena::ArenaProgram, expression: crate::syntax::arena::ExprId, ty: &super::Type) {
        use crate::syntax::arena::{ArenaExprKind as Expr, ArenaRecordFieldKind as Field, ArenaCallArgKind as Argument};
        use super::ProducerPathComponent as Path;
        if !self.graph_generation { return; }
        let identity = self.expression_identity(arena, expression);
        if !self.generic.borrow().facts.expressions.contains_key(&identity)
            && !self.pending_constructor_expressions.contains_key(&identity) {
            if ty.contains_inference() { return; }
            let Ok(ty) = self.graph_type(ty, arena.arena.expr(expression).span) else { return; };
            let mut state = self.generic.borrow_mut();
            state.facts.expressions.insert(identity, ty);
            if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(identity, owner); }
        }
        if self.generic.borrow().facts.expression_producer_flows.contains_key(&identity) { return; }
        let expression_flow = |checker: &Self, child| checker.generic.borrow().facts.expression_producer_flows.get(&checker.expression_identity(arena, child)).copied();
        let kind = match arena.arena.expr(expression).kind {
            _ if self.generic.borrow().facts.constructor_applications.contains_key(&identity) => {
                let application = self.generic.borrow().facts.constructor_applications[&identity].clone();
                if self.prepared_constants.values.contains_key(&expression) {
                    ProducerFlowKind::Empty
                } else if matches!(application.authority, super::ConstructorAuthority::Record { .. }) {
                    let mut entries = Vec::with_capacity(application.supplied.len());
                    for argument in application.supplied {
                        let Some(label) = application.parameters.get(argument.slot).and_then(|parameter| parameter.label) else { return; };
                        let (origin, projection) = match argument.value {
                            super::ConstructorValueSource::Expression(origin) => (origin, None),
                            super::ConstructorValueSource::RecordField { record, field } => (record, Some(ProducerPath(vec![Path::RecordField(field)]))),
                        };
                        let existing = self.generic.borrow().facts.expression_producer_flows.get(&origin).copied();
                        let mut input = match existing {
                            Some(input) => input,
                            None => {
                                let Some(input) = self.push_source_producer_flow(ProducerFlowSource::Expression(origin), ProducerFlowKind::Opaque, arena.arena.expr(expression).span) else { return; };
                                input
                            }
                        };
                        if let Some(path) = projection {
                            let Some(projected) = self.push_source_producer_flow(ProducerFlowSource::Expression(origin), ProducerFlowKind::Project { input, path }, arena.arena.expr(expression).span) else { return; };
                            input = projected;
                        }
                        entries.push(ProducerFlowField { path: ProducerPath(vec![Path::RecordField(label)]), input });
                    }
                    ProducerFlowKind::Aggregate { entries }
                } else {
                    ProducerFlowKind::Opaque
                }
            }
            Expr::Binary { op: crate::syntax::node::BinaryOp::ResultFallback, left, right } if self.generic.borrow().facts.operations.contains_key(&identity) => {
                let requirement = self.generic.borrow().facts.operations[&identity].requirement;
                let (Some(left), Some(right)) = (expression_flow(self, left), expression_flow(self, right)) else { return; };
                match self.language_fallback_producer_flow_kind(requirement, left, right) {
                    Ok(kind) => kind,
                    Err(error) => { self.graph_error(arena.arena.expr(expression).span, error); return; }
                }
            }
            Expr::Unary { .. } | Expr::Binary { .. } if self.generic.borrow().facts.operations.contains_key(&identity) => ProducerFlowKind::Empty,
            Expr::Binary { left, right, .. } if self.generic.borrow().facts.additions.contains_key(&identity) => {
                let requirement = self.generic.borrow().facts.additions[&identity];
                let (Some(left), Some(right)) = (expression_flow(self, left), expression_flow(self, right)) else { return; };
                ProducerFlowKind::Addition { requirement, left, right }
            }
            Expr::Item => if let Some(input) = self.stream_items.last().and_then(|item| item.producer_flow) {
                ProducerFlowKind::Join { inputs: vec![input] }
            } else { ProducerFlowKind::Opaque },
            Expr::Ident(name) => if let Some(flow) = self.lookup(name).and_then(|binding| binding.producer_flow) {
                if let Some((identity, version)) = self.lookup(name).and_then(|binding| binding.producer_binding) {
                    ProducerFlowKind::CapturedBinding { identity, version, input: flow }
                } else { ProducerFlowKind::Join { inputs: vec![flow] } }
            } else if let Some(declaration) = self.graph_callable_target(arena, expression).and_then(|callable| callable.declaration) {
                let Some(origin) = self.generic.borrow().facts.expressions.get(&identity).copied() else { return; };
                ProducerFlowKind::Callable { declaration, origin: Some(origin) }
            } else if self.lookup(name).is_some() { ProducerFlowKind::Opaque } else { ProducerFlowKind::Empty },
            Expr::Field { .. } if self.graph_callable_target(arena, expression).and_then(|callable| callable.declaration).is_some() => {
                let declaration = self.graph_callable_target(arena, expression).and_then(|callable| callable.declaration).unwrap();
                let Some(origin) = self.generic.borrow().facts.expressions.get(&identity).copied() else { return; };
                ProducerFlowKind::Callable { declaration, origin: Some(origin) }
            }
            Expr::Field { base, name } if name == "call" && self.generic.borrow().facts.expression_callables.contains_key(&self.expression_identity(arena, base)) => ProducerFlowKind::Join { inputs: expression_flow(self, base).into_iter().collect() },
            Expr::Field { base, name } => if let Some(input) = expression_flow(self, base) {
                ProducerFlowKind::Project { input, path: ProducerPath(vec![Path::RecordField(name)]) }
            } else { ProducerFlowKind::Empty },
            Expr::NullSafeField { base, name } => {
                let Some(input) = expression_flow(self, base) else { return; };
                let receiver = self.expr_types.get(&arena.arena.expr(base).span).cloned().map(|ty| self.resolved_graph_view(ty));
                let (prefix, optional) = match receiver {
                    Some(super::Type::Optional(_)) => (Path::OptionalPayload, true),
                    Some(super::Type::Result(_, _)) => {
                        self.record_error_boundary_producer_input(arena, base);
                        (Path::ResultSuccess, false)
                    }
                    _ => {
                        if let Some(flow) = self.push_source_producer_flow(ProducerFlowSource::Expression(identity), ProducerFlowKind::Opaque, arena.arena.expr(expression).span) {
                            self.generic.borrow_mut().facts.expression_producer_flows.insert(identity, flow);
                        }
                        return;
                    }
                };
                let projection = ProducerFlowKind::Project { input, path: ProducerPath(vec![prefix, Path::RecordField(name)]) };
                if optional {
                    let Some(input) = self.push_source_producer_flow(ProducerFlowSource::Expression(identity), projection, arena.arena.expr(expression).span) else { return; };
                    ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![Path::OptionalPayload]), input }] }
                } else { projection }
            }
            Expr::Index { base, guarded, .. } => {
                let Some(input) = expression_flow(self, base) else { return; };
                let receiver = self.expr_types.get(&arena.arena.expr(base).span).cloned().map(|ty| self.resolved_graph_view(ty));
                let (receiver, prefix, optional) = match receiver {
                    Some(super::Type::Optional(inner)) if guarded => (Some(*inner), Some(Path::OptionalPayload), true),
                    Some(super::Type::Result(inner, _)) if guarded => (Some(*inner), Some(Path::ResultSuccess), false),
                    receiver => (receiver, None, false),
                };
                if prefix == Some(Path::ResultSuccess) { self.record_error_boundary_producer_input(arena, base); }
                let operation = self.generic.borrow().facts.operations.get(&identity).cloned();
                if let Some(operation) = operation {
                    let alternatives = (|| {
                        let state = self.generic.borrow();
                        let crate::sema::inference::RequirementTemplate::Operation { family, .. } = state.facts.graph.requirement_template(operation.requirement)? else { return Err(InferenceError::InvalidScheme); };
                        state.facts.graph.family(family)?.iter().map(|&candidate| {
                            let component = match state.language_operations.metadata(&state.facts.graph, candidate)?.operation {
                                crate::sema::operation_graph::PreparedLanguageOperation::Index { map } => if map { Path::MapValue } else { Path::ListItem },
                                crate::sema::operation_graph::PreparedLanguageOperation::ConstantKeyProjection { field } => Path::RecordField(field),
                                _ => return Err(InferenceError::InvalidScheme),
                            };
                            let mut path = prefix.iter().cloned().collect::<Vec<_>>();
                            path.push(component);
                            Ok(ProducerFlowOperationAlternative { candidate, path: None, opaque: false, transfers: vec![ProducerFlowOperationTransfer {
                                input, input_path: ProducerPath(path), output_path: ProducerPath(if optional { vec![Path::OptionalPayload] } else { Vec::new() }),
                            }] })
                        }).collect::<Result<Vec<_>, InferenceError>>()
                    })();
                    match alternatives {
                        Ok(alternatives) => {
                            let kind = ProducerFlowKind::Operation { requirement: operation.requirement, alternatives, outputs: super::ProducerEffects { pull: crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY), close: crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY) } };
                            if let Some(flow) = self.push_source_producer_flow(ProducerFlowSource::Expression(identity), kind, arena.arena.expr(expression).span) { self.generic.borrow_mut().facts.expression_producer_flows.insert(identity, flow); }
                            return;
                        }
                        Err(error) => { self.graph_error(arena.arena.expr(expression).span, error); return; }
                    }
                }
                let component = match receiver {
                    Some(super::Type::List(_)) => Some(Path::ListItem),
                    Some(super::Type::Map(_, _)) => Some(Path::MapValue),
                    _ => self.projections.get(&arena.arena.expr(expression).span).map(|projection| Path::RecordField(projection.field)),
                };
                if let Some(component) = component {
                    let mut path = Vec::new();
                    path.extend(prefix);
                    path.push(component);
                    let projection = ProducerFlowKind::Project { input, path: ProducerPath(path) };
                    if optional {
                        let Some(input) = self.push_source_producer_flow(ProducerFlowSource::Expression(identity), projection, arena.arena.expr(expression).span) else { return; };
                        ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![Path::OptionalPayload]), input }] }
                    } else { projection }
                } else { ProducerFlowKind::Opaque }
            }
            Expr::Slice { base, .. } => ProducerFlowKind::Join { inputs: expression_flow(self, base).into_iter().collect() },
            Expr::Require { value, .. } => {
                let input = match expression_flow(self, value) {
                    Some(input) => input,
                    None => {
                        let source = ProducerFlowSource::Expression(self.expression_identity(arena, value));
                        let Some(input) = self.push_source_producer_flow(source, ProducerFlowKind::Opaque, arena.arena.expr(value).span) else { return; };
                        input
                    }
                };
                ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![Path::ResultSuccess]), input }] }
            }
            Expr::Capture(block) => {
                let input = match self.block_producer_flow(arena, block) {
                    Some(input) => input,
                    None => {
                        let Some(input) = self.push_source_producer_flow(ProducerFlowSource::Expression(identity), ProducerFlowKind::Opaque, arena.arena.expr(expression).span) else { return; };
                        input
                    }
                };
                ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![Path::ResultSuccess]), input }] }
            }
            Expr::Try(inner) => {
                self.record_error_boundary_producer_input(arena, inner);
                let Some(input) = expression_flow(self, inner) else { return; };
                ProducerFlowKind::Project { input, path: ProducerPath(vec![Path::ResultSuccess]) }
            }
            Expr::ListComp { expr, .. } => {
                let Some(input) = expression_flow(self, expr) else { return; };
                ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![Path::ListItem]), input }] }
            }
            Expr::MapComp { key, value, .. } => {
                let (Some(key), Some(value)) = (expression_flow(self, key), expression_flow(self, value)) else { return; };
                ProducerFlowKind::Aggregate { entries: vec![
                    ProducerFlowField { path: ProducerPath(vec![Path::MapKey]), input: key },
                    ProducerFlowField { path: ProducerPath(vec![Path::MapValue]), input: value },
                ] }
            }
            Expr::List(elements) => {
                let mut entries = Vec::new();
                for element in arena.arena.list_elements(elements) {
                    let Some(mut input) = expression_flow(self, element.value) else { continue; };
                    if element.splice_span.is_some() {
                        let Some(projected) = self.push_source_producer_flow(ProducerFlowSource::Expression(identity), ProducerFlowKind::Project { input, path: ProducerPath(vec![Path::ListItem]) }, arena.arena.expr(element.value).span) else { return; };
                        input = projected;
                    }
                    entries.push(ProducerFlowField { path: ProducerPath(vec![Path::ListItem]), input });
                }
                ProducerFlowKind::Aggregate { entries }
            }
            Expr::Record(_) if self.generic.borrow().facts.record_updates.contains_key(&identity) => {
                let state = self.generic.borrow();
                let update = &state.facts.record_updates[&identity];
                let Some(&base) = state.facts.expression_producer_flows.get(&update.base) else { return; };
                let replacements = update.replacements.iter().map(|replacement| ProducerFlowField {
                    path: ProducerPath(replacement.path.iter().copied().map(Path::RecordField).collect()), input: replacement.producer_flow,
                }).collect();
                ProducerFlowKind::RecordUpdate { base, replacements }
            }
            Expr::Record(fields) => {
                let mut entries = Vec::new();
                for field in arena.arena.record_fields(fields) {
                    let (path, flow) = match field.kind {
                        Field::Named { name, value, .. } => (vec![Path::RecordField(name)], expression_flow(self, value)),
                        Field::Path { path, value, .. } => (arena.arena.names(path).map(Path::RecordField).collect(), expression_flow(self, value)),
                        Field::Shorthand { name, .. } => (vec![Path::RecordField(name)], self.lookup(name).and_then(|binding| binding.producer_flow)),
                        Field::Spread { expr, .. } => (Vec::new(), expression_flow(self, expr)),
                        Field::Computed { key, value, .. } => {
                            for (component, child) in [(Path::MapKey, key), (Path::MapValue, value)] {
                                if let Some(input) = expression_flow(self, child) { entries.push(ProducerFlowField { path: ProducerPath(vec![component]), input }); }
                            }
                            continue;
                        },
                    };
                    if let Some(input) = flow { entries.push(ProducerFlowField { path: ProducerPath(path), input }); }
                }
                ProducerFlowKind::Aggregate { entries }
            }
            Expr::If { branches, else_value } => {
                let children = arena.arena.if_expr_branches(branches).iter().map(|branch| branch.value).chain(std::iter::once(else_value));
                let inputs = children.filter_map(|child| expression_flow(self, child)).collect();
                let joined = ProducerFlowKind::Join { inputs };
                if matches!(self.resolved_graph_view(ty.clone()), super::Type::Optional(_)) {
                    let Some(input) = self.push_source_producer_flow(ProducerFlowSource::Expression(identity), joined, arena.arena.expr(expression).span) else { return; };
                    ProducerFlowKind::OptionalLift { input }
                } else { joined }
            }
            Expr::Match { arms, .. } => ProducerFlowKind::Join { inputs: arena.arena.match_expr_arms(arms).iter().filter_map(|arm| expression_flow(self, arm.value)).collect() },
            Expr::ValueBlock(block) => ProducerFlowKind::Join { inputs: self.block_producer_flow(arena, block).into_iter().collect() },
            Expr::Call { args, .. } if self.generic.borrow().facts.operations.get(&identity).is_some_and(|operation| {
                let state = self.generic.borrow();
                matches!(state.facts.graph.requirement_template(operation.requirement), Ok(crate::sema::inference::RequirementTemplate::Operation { family, .. }) if state.facts.graph.family(family).is_ok_and(|candidates| candidates.iter().all(|candidate| matches!(state.language_operations.metadata(&state.facts.graph, *candidate), Ok(metadata) if matches!(metadata.operation, crate::sema::operation_graph::PreparedLanguageOperation::Constructor { .. })))))
            }) => {
                let state = self.generic.borrow();
                let requirement = state.facts.operations[&identity].requirement;
                let crate::sema::inference::RequirementTemplate::Operation { family, .. } = state.facts.graph.requirement_template(requirement).unwrap() else { unreachable!() };
                let alternatives = state.facts.graph.family(family).unwrap().iter().map(|&candidate| {
                    let metadata = state.language_operations.metadata(&state.facts.graph, candidate).unwrap();
                    let crate::sema::operation_graph::PreparedLanguageOperation::Constructor { kind, arity } = metadata.operation else { unreachable!() };
                    use crate::sema::operation_graph::ValueConstructor;
                    let path = (kind == ValueConstructor::Range).then(ProducerPath::default);
                    let mut transfers = Vec::new();
                    let mut opaque = false;
                    if arity > 0 && matches!(kind, ValueConstructor::Ok | ValueConstructor::Err) {
                        let source = state.facts.operations[&identity].binding.supplied_slots.iter().position(|&slot| slot == 0).and_then(|index| arena.arena.call_args(args).get(index));
                        let input = source.and_then(|argument| {
                            let child = match argument.kind { Argument::Positional(child) | Argument::Named { value: child, .. } | Argument::NamedSpread { value: child, .. } | Argument::Splice { value: child, .. } => child };
                            expression_flow(self, child)
                        });
                        if let Some(input) = input { transfers.push(ProducerFlowOperationTransfer { input, input_path: ProducerPath::default(), output_path: ProducerPath(vec![if kind == ValueConstructor::Ok { Path::ResultSuccess } else { Path::ResultError }]) }); }
                        else { opaque = true; }
                    }
                    ProducerFlowOperationAlternative { candidate, path, transfers, opaque }
                }).collect();
                ProducerFlowKind::Operation { requirement, alternatives, outputs: super::ProducerEffects { pull: crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY), close: crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY) } }
            }
            Expr::Call { callee, .. } if self.generic.borrow().facts.calls.contains_key(&identity) || self.generic.borrow().facts.invocations.contains_key(&identity) => {
                let callee = if self.generic.borrow().facts.invocations.contains_key(&identity) {
                    match arena.arena.expr(callee).kind { Expr::Field { base, name } if name == "call" => base, _ => callee }
                } else { callee };
                if expression_flow(self, callee).is_none() && let Some(callable) = self.graph_callable_target(arena, callee) {
                    if let Some(scheme) = callable.scheme {
                        let identity = self.expression_identity(arena, callee);
                        self.generic.borrow_mut().facts.expression_schemes.insert(identity, scheme);
                    }
                    self.record_graph_expression(arena, callee, &super::Type::Graph(callable.signature));
                    self.record_expression_producer_flow(arena, callee, &super::Type::Graph(callable.signature));
                }
                let Some(callee) = expression_flow(self, callee) else { return; };
                let Some(sources) = self.generic.borrow().facts.argument_sources.get(&identity).cloned() else { return; };
                let mut arguments = Vec::with_capacity(sources.len());
                for source in sources {
                    use crate::sema::arguments::ArgumentValueSource as Source;
                    let (expression, path) = match source.value {
                        Source::Expression(expression) => (expression, None),
                        Source::RecordField { record, field } => (record, Some(ProducerPath(vec![Path::RecordField(field)]))),
                        Source::PositionalSplice(expression) => (expression, None),
                    };
                    let Some(input) = expression_flow(self, expression) else { return; };
                    let flow = if let Some(path) = path {
                        let source = ProducerFlowSource::Expression(self.expression_identity(arena, expression));
                        let Some(flow) = self.push_source_producer_flow(source, ProducerFlowKind::Project { input, path }, arena.arena.expr(expression).span) else { return; };
                        flow
                    } else { input };
                    arguments.push(flow);
                }
                ProducerFlowKind::Apply { call: identity, callee, arguments }
            }
            Expr::Null | Expr::Bool(_) | Expr::Int(_) | Expr::Float(_) | Expr::Duration(_) | Expr::Str(_) | Expr::FmtString(_) | Expr::PathFmtString(_) | Expr::PathStr(_) | Expr::Bytes(_) | Expr::Regex(_) => ProducerFlowKind::Empty,
            _ => return,
        };
        if let Some(flow) = self.push_source_producer_flow(ProducerFlowSource::Expression(identity), kind, arena.arena.expr(expression).span) {
            let mut state = self.generic.borrow_mut();
            state.facts.expression_producer_flows.insert(identity, flow);
            if let Some(call) = state.facts.calls.get_mut(&identity) { call.result_producer_flow = Some(flow); }
        }
    }

    /// Failure payloads retain their source value paths separately from the
    /// success continuation. Nested captures own separate reached-failure lists.
    pub(super) fn record_error_boundary_producer_input(&mut self, arena: &crate::syntax::arena::ArenaProgram, expression: crate::syntax::arena::ExprId) {
        if !self.graph_generation || self.error_boundary_producer_flows.is_empty() && self.current_generic.is_none() { return; }
        let identity = self.expression_identity(arena, expression);
        let input = self.generic.borrow().facts.expression_producer_flows.get(&identity).copied();
        let Some(input) = input else { return; };
        self.record_error_boundary_producer_flow(ProducerFlowSource::Expression(identity), input, arena.arena.expr(expression).span);
    }

    fn record_error_boundary_producer_flow(&mut self, source: ProducerFlowSource, input: ProducerFlowId, span: crate::source::Span) {
        if !self.graph_generation || self.error_boundary_producer_flows.is_empty() && self.current_generic.is_none() { return; }
        let kind = ProducerFlowKind::Project { input, path: ProducerPath(vec![super::ProducerPathComponent::ResultError]) };
        if let Some(flow) = self.push_source_producer_flow(source, kind, span) {
            if let Some(errors) = self.error_boundary_producer_flows.last_mut() { errors.push(flow); }
            else if let Some(owner) = self.current_generic { self.generic.borrow_mut().pending.get_mut(&owner).unwrap().propagated_error_producer_flows.push(flow); }
        }
    }

    pub(super) fn record_capture_completion_producer_flow(&mut self, arena: &crate::syntax::arena::ArenaProgram, block: crate::syntax::arena::BlockId, forwards_result: bool, span: crate::source::Span) {
        if !self.graph_generation { return; }
        let Some(expression) = self.current_expression else { return; };
        let identity = self.expression_identity(arena, expression);
        let source = ProducerFlowSource::Expression(identity);
        let input = match self.block_producer_flow(arena, block) {
            Some(input) => input,
            None => { let Some(input) = self.push_source_producer_flow(source, ProducerFlowKind::Opaque, span) else { return; }; input },
        };
        let count = self.error_boundary_producer_flows.last().map_or(0, Vec::len);
        let charge = self.generic.borrow_mut().facts.graph.charge_source_fact_work(count as u64 + 1);
        if let Err(error) = charge { self.graph_error(span, error); return; }
        let errors = self.error_boundary_producer_flows.last().cloned().unwrap_or_default();
        let mut entries = errors.into_iter().map(|input| ProducerFlowField { path: ProducerPath(vec![super::ProducerPathComponent::ResultError]), input }).collect::<Vec<_>>();
        let kind = if forwards_result {
            let mut inputs = vec![input];
            if !entries.is_empty() {
                let Some(errors) = self.push_source_producer_flow(source, ProducerFlowKind::Aggregate { entries }, span) else { return; };
                inputs.push(errors);
            }
            ProducerFlowKind::Join { inputs }
        } else {
            entries.push(ProducerFlowField { path: ProducerPath(vec![super::ProducerPathComponent::ResultSuccess]), input });
            ProducerFlowKind::Aggregate { entries }
        };
        if let Some(flow) = self.push_source_producer_flow(source, kind, span) { self.generic.borrow_mut().facts.expression_producer_flows.insert(identity, flow); }
    }

    pub(super) fn block_producer_flow(&self, arena: &crate::syntax::arena::ArenaProgram, block: crate::syntax::arena::BlockId) -> Option<ProducerFlowId> {
        let statement = arena.arena.stmt_ids(arena.arena.block(block).statements).last()?;
        let identity = StatementIdentity { source: arena.arena.stmt(statement).span.source_id, namespace: self.current_namespace, statement };
        self.generic.borrow().facts.statement_producer_flows.get(&identity).copied()
    }

    pub(super) fn record_statement_producer_flow(&mut self, arena: &crate::syntax::arena::ArenaProgram, statement: crate::syntax::arena::StmtId) {
        use crate::syntax::arena::ArenaStmtKind as Stmt;
        if !self.graph_generation { return; }
        let identity = StatementIdentity { source: arena.arena.stmt(statement).span.source_id, namespace: self.current_namespace, statement };
        if self.generic.borrow().facts.statement_producer_flows.contains_key(&identity) { return; }
        let consumed = self.generic.borrow().facts.statements.get(&identity) == Some(&super::StatementPosition::Statement);
        if consumed {
            match arena.arena.stmt(statement).kind {
                Stmt::Expr(expression) => {
                    let ty = self.expr_types.get(&arena.arena.expr(expression).span).cloned().map(|ty| self.resolved_graph_view(ty));
                    if ty.as_ref().is_some_and(super::expr::expr_ty_auto_propagates) { self.record_error_boundary_producer_input(arena, expression); }
                }
                Stmt::TailBareIdent(name) => {
                    let binding = self.lookup(name).cloned();
                    if let Some(binding) = binding {
                        let ty = self.resolved_graph_view(binding.ty);
                        if super::expr::expr_ty_auto_propagates(&ty) && let Some(input) = binding.producer_flow {
                            self.record_error_boundary_producer_flow(ProducerFlowSource::Statement(identity), input, arena.arena.stmt(statement).span);
                        }
                    }
                }
                _ => {}
            }
        }
        let inputs = match arena.arena.stmt(statement).kind {
            Stmt::TailBareIdent(name) => self.lookup(name).and_then(|binding| binding.producer_flow).into_iter().collect(),
            Stmt::Expr(expression) | Stmt::Return(Some(crate::syntax::arena::ArenaExprOrRun::Expr(expression))) => self.generic.borrow().facts.expression_producer_flows.get(&self.expression_identity(arena, expression)).copied().into_iter().collect(),
            Stmt::Return(Some(crate::syntax::arena::ArenaExprOrRun::Run(run))) => self.graph_run_flow(arena, run).into_iter().collect(),
            Stmt::If { branches, else_block } => arena.arena.if_branches(branches).iter().map(|branch| branch.block).chain(else_block).filter_map(|block| self.block_producer_flow(arena, block)).collect(),
            Stmt::Match { arms, .. } => arena.arena.match_arms(arms).iter().filter_map(|arm| self.block_producer_flow(arena, arm.block)).collect(),
            _ => return,
        };
        let wrap = {
            let state = self.generic.borrow();
            state.facts.result_statement_wrappings.contains_key(&identity) || match arena.arena.stmt(statement).kind {
                Stmt::Expr(expression) | Stmt::Return(Some(crate::syntax::arena::ArenaExprOrRun::Expr(expression))) => state.facts.result_wrappings.contains_key(&self.expression_identity(arena, expression)),
                _ => false,
            }
        };
        let source = ProducerFlowSource::Statement(identity);
        let span = arena.arena.stmt(statement).span;
        let mut kind = ProducerFlowKind::Join { inputs };
        if wrap {
            let Some(input) = self.push_source_producer_flow(source, kind, span) else { return; };
            kind = ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![super::ProducerPathComponent::ResultSuccess]), input }] };
        }
        if let Some(flow) = self.push_source_producer_flow(source, kind, span) {
            self.generic.borrow_mut().facts.statement_producer_flows.insert(identity, flow);
        }
    }

    pub(super) fn record_binding_producer_flow(&mut self, arena: &crate::syntax::arena::ArenaProgram, target: crate::syntax::arena::BindingTargetId, initializer: crate::syntax::arena::ArenaExprOrRun, span: crate::source::Span) {
        if !self.graph_generation { return; }
        let crate::syntax::arena::ArenaBindingTargetKind::Name(name) = arena.arena.binding_target(target).kind else { return; };
        if name == "_" { return; }
        let input = match initializer {
            crate::syntax::arena::ArenaExprOrRun::Expr(expression) => self.generic.borrow().facts.expression_producer_flows.get(&self.expression_identity(arena, expression)).copied(),
            crate::syntax::arena::ArenaExprOrRun::Run(run) => self.graph_run_flow(arena, run),
        };
        let Some(input) = input else { return; };
        let identity = BindingIdentity { source: span.source_id, namespace: self.current_namespace, target };
        if !self.generic.borrow().facts.bindings.contains_key(&identity) { return; }
        let Some(flow) = self.push_source_producer_flow(ProducerFlowSource::Binding { identity, version: 0 }, ProducerFlowKind::Join { inputs: vec![input] }, span) else { return; };
        self.generic.borrow_mut().facts.binding_producer_flows.insert((identity, 0), flow);
        if let Some(binding) = self.current_scope_mut().get_mut(&name) { binding.producer_flow = Some(flow); binding.producer_binding = Some((identity, 0)); }
        if let Some(value) = self.evaluate_source_producer_flow(flow, span) && !value.pending && value.pending_paths.is_empty() && value.opaque_paths.is_empty() {
            let mut state = self.generic.borrow_mut();
            if let crate::syntax::arena::ArenaExprOrRun::Expr(expression) = initializer {
                state.facts.expression_producers.insert(self.expression_identity(arena, expression), value.profile.clone());
            }
            state.facts.binding_producers.insert(identity, value.profile);
        }
    }

    pub(super) fn record_assignment_producer_flow(&mut self, arena: &crate::syntax::arena::ArenaProgram, target: crate::syntax::arena::AssignTargetId, value: crate::syntax::arena::ArenaExprOrRun, span: crate::source::Span) {
        if !self.graph_generation { return; }
        let crate::syntax::arena::ArenaAssignTargetKind::Name(name) = arena.arena.assign_target(target).kind else { return; };
        let Some(binding) = self.lookup(name) else { return; };
        if !binding.mutable { return; }
        let Some((identity, _)) = binding.producer_binding else { return; };
        let input = match value {
            crate::syntax::arena::ArenaExprOrRun::Expr(expression) => self.generic.borrow().facts.expression_producer_flows.get(&self.expression_identity(arena, expression)).copied(),
            crate::syntax::arena::ArenaExprOrRun::Run(run) => self.graph_run_flow(arena, run),
        };
        let Some(input) = input else { return; };
        let version = {
            let mut state = self.generic.borrow_mut();
            let next = state.producer_binding_versions.entry(identity).or_insert(1);
            let version = *next;
            let Some(after) = next.checked_add(1) else { drop(state); self.graph_error(span, InferenceError::Limit("producer binding versions")); return; };
            *next = after;
            version
        };
        let Some(flow) = self.push_source_producer_flow(ProducerFlowSource::Binding { identity, version }, ProducerFlowKind::Join { inputs: vec![input] }, span) else { return; };
        self.generic.borrow_mut().facts.binding_producer_flows.insert((identity, version), flow);
        if let Some(binding) = self.scopes.iter_mut().rev().find_map(|scope| scope.get_mut(&name)) {
            binding.producer_flow = Some(flow);
            binding.producer_binding = Some((identity, version));
        }
    }

    pub(super) fn finish_declaration_producer_flow(&mut self, arena: &crate::syntax::arena::ArenaProgram, def: &crate::syntax::arena::ArenaFunctionDef, identity: DeclarationIdentity) {
        if !self.graph_generation || self.generic.borrow().pending[&identity].return_producer_flow.is_some() { return; }
        let mut inputs = self.generic.borrow().pending[&identity].completion_producer_flows.clone();
        inputs.extend(self.block_producer_flow(arena, def.body));
        let source = ProducerFlowSource::DeclarationResult(identity);
        let producer = self.generic.borrow().pending[&identity].producer_effects;
        let kind = if let Some(effects) = producer {
            let Some(root) = self.push_source_producer_flow(source, ProducerFlowKind::Known(std::collections::BTreeMap::from([(ProducerPath::default(), effects)])), arena.arena.span(arena.arena.block(def.body).span)) else { return; };
            let entries = inputs.into_iter().map(|input| ProducerFlowField { path: ProducerPath(vec![super::ProducerPathComponent::ListItem]), input }).collect();
            let Some(items) = self.push_source_producer_flow(source, ProducerFlowKind::Aggregate { entries }, arena.arena.span(arena.arena.block(def.body).span)) else { return; };
            ProducerFlowKind::Join { inputs: vec![root, items] }
        } else { ProducerFlowKind::Join { inputs } };
        let span = arena.arena.span(arena.arena.block(def.body).span);
        let Some(mut flow) = self.push_source_producer_flow(source, kind, span) else { return; };
        let implicit = self.generic.borrow().pending[&identity].return_elaboration == Some(super::ReturnElaboration::ImplicitResult);
        if producer.is_none() && implicit {
            let kind = ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![super::ProducerPathComponent::ResultSuccess]), input: flow }] };
            let Some(wrapped) = self.push_source_producer_flow(source, kind, span) else { return; };
            flow = wrapped;
        }
        let result = self.generic.borrow().pending[&identity].result;
        if producer.is_none() && matches!(self.resolved_graph_view(self.graph_view(result)), super::Type::Optional(_)) {
            let Some(lifted) = self.push_source_producer_flow(source, ProducerFlowKind::OptionalLift { input: flow }, span) else { return; };
            flow = lifted;
        }
        if producer.is_none() && matches!(self.resolved_graph_view(self.graph_view(result)), super::Type::Result(_, _)) {
            let count = self.generic.borrow().pending[&identity].propagated_error_producer_flows.len();
            let charge = self.generic.borrow_mut().facts.graph.charge_source_fact_work(count as u64 + 1);
            if let Err(error) = charge { self.graph_error(span, error); return; }
            if count > 0 {
                let errors = self.generic.borrow().pending[&identity].propagated_error_producer_flows.clone();
                let entries = errors.into_iter().map(|input| ProducerFlowField { path: ProducerPath(vec![super::ProducerPathComponent::ResultError]), input }).collect();
                let Some(errors) = self.push_source_producer_flow(source, ProducerFlowKind::Aggregate { entries }, span) else { return; };
                let Some(joined) = self.push_source_producer_flow(source, ProducerFlowKind::Join { inputs: vec![flow, errors] }, span) else { return; };
                flow = joined;
            }
        }
        self.generic.borrow_mut().pending.get_mut(&identity).unwrap().return_producer_flow = Some(flow);
        if let Some(declaration) = self.generic.borrow_mut().producer_inputs.declarations.get_mut(&identity) { declaration.result = Some(flow); }
    }
}

pub(super) fn instantiate_producer_profile(graph: &mut InferenceContext, profile: &ProducerProfile, scheme: crate::sema::inference::SchemeId, substitutions: &[crate::sema::inference::EffectSummary]) -> Result<ProducerProfile, InferenceError> {
    let binders = graph.scheme_effect_binders(scheme)?;
    let replace = |graph: &mut InferenceContext, effect| {
        graph.charge_source_fact_work(binders.len() as u64 + 1)?;
        let effect = graph.resolved_effect_summary(effect)?;
        if let Some(index) = binders.iter().position(|binder| *binder == effect) {
            substitutions.get(index).copied().ok_or(InferenceError::InvalidScheme)
        } else { Ok(effect) }
    };
    let mut instantiated = ProducerProfile::new();
    for (path, effects) in profile {
        graph.charge_source_fact_work(path.0.len() as u64 + 1)?;
        instantiated.insert(path.clone(), super::ProducerEffects { pull: replace(graph, effects.pull)?, close: replace(graph, effects.close)? });
    }
    Ok(instantiated)
}
