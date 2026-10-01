use super::{CallBinding, Checker, ProducerEffects, ProducerFlowId, ProducerFlowKind, ProducerFlowSource, ProducerPath, ProducerPathComponent, SolvedOperation, SolvedStage, StageCallback, StageIdentity, Type};
use crate::sema::arguments::{ArgumentValueSource, ExpandedArgument};
use crate::sema::inference::{Arrow, CallableDomain, CallableKind, EffectRole, EffectSet, EffectSummary, InferenceError, InvocationArgument, InvocationArgumentKind, InvocationCall, OperationCall, Parameter, TypeId, TypeNode};
use crate::sema::stage_graph::{CallbackShape, ReductionModes, SequenceDomain, StageForm, StageSource, sequence_effect_roles};
use crate::syntax::arena::{ArenaCallArgKind, ArenaExprKind, ArenaProgram, ArenaRange, ArenaStreamStage, ExprId};
use crate::syntax::node::StreamStageKind;
use crate::source::Span;
use crate::symbol::Name;

fn terminal(stage: &StreamStageKind) -> bool {
    matches!(stage, StreamStageKind::Each | StreamStageKind::First | StreamStageKind::Last | StreamStageKind::Sum | StreamStageKind::Min | StreamStageKind::Max | StreamStageKind::Fold | StreamStageKind::Reduce | StreamStageKind::Any | StreamStageKind::All | StreamStageKind::Count | StreamStageKind::Collect | StreamStageKind::ReduceBy | StreamStageKind::TablePrint)
}

#[derive(Clone, Copy)]
enum StageItemSource { Input, ZipArgument, CallbackResult }

impl Checker {
    pub(super) fn check_graph_pipeline_stages(&mut self, arena: &ArenaProgram, source: &str, input: ExprId, stages: ArenaRange) -> Type {
        let Some(pipeline) = self.current_expression else {
            self.graph_error(arena.arena.expr(input).span, InferenceError::Boundary("structured stages require their source pipeline identity"));
            return Type::Invalid;
        };
        let pipeline_identity = self.expression_identity(arena, pipeline);
        if !self.graph_generation {
            return self.generic.borrow().facts.expressions.get(&pipeline_identity).map(|ty| self.graph_view(*ty)).unwrap_or(Type::Invalid);
        }
        let input_type = self.check_expr_arena(arena, source, input, None);
        if stages.is_empty() { return input_type; }
        let mut current = input_type;
        let mut flow = self.generic.borrow().facts.expression_producer_flows.get(&self.expression_identity(arena, input)).copied();
        for (ordinal, stage) in arena.arena.stream_stages(stages).iter().enumerate() {
            let identity = StageIdentity { pipeline: pipeline_identity, index: stages.start + ordinal as u32 };
            if ordinal > 0 && stage.kind.is_adapter() {
                self.error(arena.arena.span(stage.span), "adapter stages are valid only as the first structured pipeline stage", "check.stream-adapter");
                return Type::Invalid;
            }
            let input_type = current;
            current = match self.check_source_stage(arena, source, identity, stage, &input_type, flow) {
                Ok((ty, output)) => { flow = output; ty }
                Err(error) => { self.source_stage_error(arena.arena.span(stage.span), error); return Type::Invalid; }
            };
            self.stream_stage_types.insert((self.current_namespace, arena.arena.span(stage.span)), super::CheckedStreamStage { input: input_type, output: current.clone() });
        }
        let last = arena.arena.stream_stages(stages).last().unwrap();
        if !terminal(&last.kind) {
            let span = arena.arena.span(last.span);
            let output_effects = {
                let state = self.generic.borrow();
                let identity = StageIdentity { pipeline: pipeline_identity, index: stages.start + stages.len - 1 };
                state.facts.stage_operations.get(&identity).ok_or(InferenceError::InvalidScheme)
                    .and_then(|stage| state.stage_graph.output_producer_effects(&state.facts.graph, stage.operation.requirement))
                    .and_then(|effects| effects.ok_or(InferenceError::InvalidScheme))
            };
            match output_effects {
                Ok((pull, close)) => { self.check_stage_effects(pull, span); self.check_stage_effects(close, span); }
                Err(error) => { self.graph_error(span, error); return Type::Invalid; }
            }
            let outcome = (|| {
                let ty = self.graph_type(&current, span)?;
                let mut state = self.generic.borrow_mut();
                let graph = &mut state.facts.graph;
                let TypeNode::Stream(item) = graph.node(graph.resolved(ty)?)? else { return Err(InferenceError::Boundary("nonterminal stage has no checked stream result")); };
                graph.list(*item)
            })();
            current = match outcome { Ok(ty) => self.graph_view(ty), Err(error) => { self.graph_error(span, error); return Type::Invalid; } };
        }
        // The pipeline expression is the materialized consumer. Intermediate
        // producer handles belong to their stage identities, never to this List.
        if self.graph_generation {
            let ty = match self.graph_type(&current, arena.arena.expr(pipeline).span) { Ok(ty) => ty, Err(error) => { self.graph_error(arena.arena.expr(pipeline).span, error); return Type::Invalid; } };
            self.record_graph_expression(arena, pipeline, &Type::Graph(ty));
            let result_flow = if let Some(flow) = flow {
                if !terminal(&last.kind) {
                    self.push_source_producer_flow(ProducerFlowSource::Expression(pipeline_identity), ProducerFlowKind::Project { input: flow, path: ProducerPath(vec![ProducerPathComponent::ListItem]) }, arena.arena.expr(pipeline).span)
                        .and_then(|item| self.push_source_producer_flow(ProducerFlowSource::Expression(pipeline_identity), ProducerFlowKind::Aggregate { entries: vec![super::ProducerFlowField { path: ProducerPath(vec![ProducerPathComponent::ListItem]), input: item }] }, arena.arena.expr(pipeline).span))
                } else { self.push_source_producer_flow(ProducerFlowSource::Expression(pipeline_identity), ProducerFlowKind::Join { inputs: vec![flow] }, arena.arena.expr(pipeline).span) }
            } else { self.push_source_producer_flow(ProducerFlowSource::Expression(pipeline_identity), ProducerFlowKind::Empty, arena.arena.expr(pipeline).span) };
            if let Some(flow) = result_flow { self.generic.borrow_mut().facts.expression_producer_flows.insert(pipeline_identity, flow); }
        }
        current
    }

    fn check_source_stage(&mut self, arena: &ArenaProgram, source: &str, identity: StageIdentity, stage: &ArenaStreamStage, input: &Type, input_flow: Option<ProducerFlowId>) -> Result<(Type, Option<ProducerFlowId>), InferenceError> {
        if !self.graph_generation || self.generic.borrow().facts.stage_operations.contains_key(&identity) {
            let state = self.generic.borrow();
            let fact = state.facts.stage_operations.get(&identity).ok_or(InferenceError::Boundary("stage has no established source relationship"))?;
            return Ok((self.graph_view(fact.operation.result), fact.result_producer_flow));
        }
        let span = arena.arena.span(stage.span);
        if xsh_registry::stream_parameters::stage_parameters(stage.kind.as_str()).is_empty()
            && crate::sema::stage_graph::stage_callback_presence(&stage.kind) == crate::sema::stage_graph::StageCallbackPresence::Absent
            && (!stage.args.is_empty() || stage.block.is_some()) {
            self.graph_boundary_error(span, "stream stage accepts no arguments or block", "check.arity");
            return Err(InferenceError::Boundary("stage supplied arguments exceed its canonical source form"));
        }
        // Spread fields are checked before the descriptor binder reads their
        // finite labels; the descriptor itself remains a per-item invocation.
        for argument in arena.arena.call_args(stage.args) {
            if let ArenaCallArgKind::NamedSpread { value, .. } = argument.kind { self.check_expr_arena(arena, source, value, None); }
        }
        let descriptor = match crate::sema::stage_arguments::stage_callable_argument(arena, stage, |expression| self.expr_types.get(&arena.arena.expr(expression).span).cloned()) {
            Ok(value) => value.map(|(expression, _)| expression),
            Err((span, message)) => { self.error(span, &message, "check.stream-callable"); return Err(InferenceError::Boundary("stage callable descriptor cannot be bound")); }
        };
        let arguments: Vec<_> = arena.arena.call_args(stage.args).iter().filter(|argument| !descriptor.is_some_and(|callee| matches!(argument.kind, ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. } if value == callee))).cloned().collect();
        let configuration = self.check_stage_arguments_arena(arena, source, stage, &arguments);
        let config = xsh_registry::stream_parameters::stage_parameters(stage.kind.as_str());
        let receiver = self.graph_type(input, span)?;
        let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
        let item = self.generic.borrow_mut().facts.graph.fresh(level, span)?;
        let mut supplied = Vec::with_capacity(config.len() + 1);
        let mut actual_arguments = Vec::new();
        let mut supplied_slots = Vec::new();
        let mut written_configuration = Vec::new();
        let mut default_slots = Vec::new();
        for (slot, argument) in configuration.iter().enumerate() {
            if let Some(argument) = argument {
                let ty = self.graph_type(&argument.ty, argument.span)?;
                supplied.push(Some(ty)); written_configuration.push((argument.entry_index, slot, ty));
            } else { supplied.push(None); default_slots.push(slot); }
        }
        written_configuration.sort_by_key(|(entry, _, _)| *entry);
        for (_, slot, ty) in written_configuration { actual_arguments.push(ty); supplied_slots.push(slot); }
        let mut form = StageForm::default();
        let bool_value = |slot: usize| match configuration.get(slot).and_then(|argument| argument.as_ref()) {
            None => Some(false),
            Some(argument) => self.stage_argument_literal_bool(arena, argument),
        };
        if stage.kind == StreamStageKind::Batch { form.batch_argv = configuration.get(1).is_some_and(Option::is_some) || bool_value(2) != Some(false); }
        if stage.kind == StreamStageKind::ReduceBy { form.reduction_modes = Some(ReductionModes::from_static_configuration(bool_value(0), bool_value(1), bool_value(2))?); }
        let choice_descriptor = if let Some(callee) = descriptor {
            if !self.stage_callable_is_static(arena, callee) {
                self.error(arena.arena.expr(callee).span, "stage callable must be a statically resolved named function or proc", "check.stream-callable");
                return Err(InferenceError::Boundary("stage descriptor has no static callable contract"));
            }
            self.prepare_graph_callable_value(arena, source, callee);
            if self.graph_callable_target(arena, callee).is_none()
                && let ArenaExprKind::Field { base, name } = arena.arena.expr(callee).kind
                && let ArenaExprKind::Ident(module) = arena.arena.expr(base).kind
                && self.lookup(module).is_none()
                && super::api_spec().module(&module.as_str()).is_some_and(|entry| entry.function_overloads(&name.as_str()).is_some()) {
                let family_reference = {
                    let mut state = self.generic.borrow_mut();
                    let super::generic::GenericState { facts, registry, .. } = &mut *state;
                    let family = registry.module_family(&mut facts.graph, &module.as_str(), &name.as_str(), span)?;
                    let candidates = facts.graph.family(family)?;
                    candidates.len() > 1 && candidates.iter().all(|&candidate| {
                        let Ok(template) = facts.graph.candidate(candidate) else { return false; };
                        let Ok(metadata) = registry.metadata(&facts.graph, candidate) else { return false; };
                        let Ok(scheme) = facts.graph.scheme(template.scheme) else { return false; };
                        metadata.reference_family_member_supported(template) && scheme.quantifiers.is_empty() && scheme.effect_quantifiers.is_empty()
                    })
                };
                if family_reference { self.check_graph_registry_reference(arena, callee, base, name, arena.arena.expr(callee).span); }
            }
            let choice = self.graph_callable_target(arena, callee).map(|target| {
                let state = self.generic.borrow();
                let graph = &state.facts.graph;
                let signature = graph.callable_signature(target.signature)?;
                Ok(matches!(graph.node(graph.resolved(signature)?)?, TypeNode::CallableChoice(_)))
            }).transpose()?.unwrap_or(false);
            if choice { Some(self.source_stage_protocol_descriptor(arena, identity, callee, span)?) } else { None }
        } else { None };
        let callback_kinds = if let Some((callable, _)) = choice_descriptor {
            let mut state = self.generic.borrow_mut();
            let graph = &mut state.facts.graph;
            let signature = graph.callable_signature(callable)?;
            let count = match graph.node(graph.resolved(signature)?)? {
                TypeNode::CallableChoice(signatures) => signatures.len(),
                TypeNode::Arrow(_) => 1,
                _ => return Err(InferenceError::InvalidScheme),
            };
            graph.charge_source_fact_work(count as u64)?;
            let signatures = graph.callable_signatures(callable)?;
            let mut kinds = Vec::new();
            for signature in signatures {
                let TypeNode::Arrow(arrow) = graph.node(graph.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme); };
                if arrow.kind == CallableKind::Stream || self.in_pure && arrow.kind != CallableKind::Pure { continue; }
                if !kinds.contains(&arrow.kind) { kinds.push(arrow.kind); }
            }
            if kinds.is_empty() { return Err(InferenceError::Boundary("stage callback violates its callable domain")); }
            form.callback_protocol = true;
            kinds
        } else {
            form.callback_kind = if let Some(callee) = descriptor {
                Some(self.source_stage_descriptor_kind(arena, source, callee, span)?)
            } else { stage.block.map(|_| if self.in_pure { CallableKind::Pure } else { CallableKind::Proc }) };
            form.callback_kind.into_iter().collect()
        };
        if let Some((callable, _)) = choice_descriptor { supplied.push(Some(callable)); }
        let protocol_callback = if let Some(kind) = form.callback_kind {
            let (result, effects) = {
                let mut state = self.generic.borrow_mut();
                let result = state.facts.graph.fresh(level, span)?;
                let effects = EffectSummary::Variable(state.facts.graph.fresh_effect_at(level, None)?);
                (result, effects)
            };
            let parameters = if matches!(stage.kind, StreamStageKind::Fold | StreamStageKind::Reduce) {
                vec![supplied.first().copied().flatten().ok_or(InferenceError::Boundary("reduction requires its initial value"))?, item]
            } else { vec![item] };
            let callback = self.stage_callback_shell(kind, &parameters, result, effects)?;
            supplied.push(Some(callback));
            Some(callback)
        } else { None };
        // Establish the actual protocol before checking its original callback.
        // Source item transfers refer to this requirement even when its
        // candidate remains dependent on the callback's result.
        let (requirement, result, effects, outputs, candidates, protocol_roles) = {
            let mut state = self.generic.borrow_mut();
            let super::generic::GenericState { facts, stage_graph, .. } = &mut *state;
            let family = if choice_descriptor.is_some() {
                let mut members = Vec::new();
                for &kind in &callback_kinds {
                    let family = stage_graph.family(&mut facts.graph, stage.kind.clone(), StageForm { callback_kind: Some(kind), ..form }, span)?;
                    members.extend_from_slice(facts.graph.family(family)?);
                }
                facts.graph.register_family(&members)?
            } else { stage_graph.family(&mut facts.graph, stage.kind.clone(), form, span)? };
            let candidates = facts.graph.family(family)?.to_vec();
            let role_names = facts.graph.candidate(*candidates.first().ok_or(InferenceError::InvalidScheme)?)?.effect_roles.iter().map(|(role, _)| *role).collect::<Vec<_>>();
            let roles = role_names.into_iter().map(|role| Ok((role, EffectSummary::Variable(facts.graph.fresh_effect_at(level, None)?)))).collect::<Result<Vec<_>, InferenceError>>()?;
            let outputs = stage_graph.output_effect_bindings(&mut facts.graph, family, level)?;
            let result = if terminal(&stage.kind) { facts.graph.fresh(level, span)? } else { let item = facts.graph.fresh(level, span)?; facts.graph.stream(item)? };
            let effects = EffectSummary::Variable(facts.graph.fresh_derived_effect_at(level, None)?);
            let reason = facts.graph.reason(span, None)?;
            let requirement = facts.graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: Some(receiver), arguments: supplied.clone(), result, effects, effect_bindings: roles.clone(), output_effect_bindings: outputs.clone() }, reason)?;
            facts.graph.solve()?;
            (requirement, result, effects, outputs, candidates, roles)
        };
        let item_flow = self.stage_item_transfer(identity, requirement, input_flow, StageItemSource::Input, span)?;
        let mut callback = None;
        let mut callback_flow = None;
        let mut callback_effects = None;
        let mut secondary_flow = None;
        if let Some((callable, declaration)) = choice_descriptor {
            let callee = descriptor.ok_or(InferenceError::InvalidScheme)?;
            let formal_slot = config.len() + 1;
            self.generic.borrow_mut().producer_inputs.stage_invocation_projections.insert(identity, (requirement, formal_slot));
            let callee_flow = self.generic.borrow().facts.expression_producer_flows.get(&self.expression_identity(arena, callee)).copied();
            callback_flow = if let (Some(callee), Some(item)) = (callee_flow, item_flow) {
                self.push_source_producer_flow(ProducerFlowSource::Stage(identity), ProducerFlowKind::StageApply { stage: identity, callee, arguments: vec![item] }, span)
            } else { None };
            callback_effects = Some(protocol_roles.iter().find(|(role, _)| *role == EffectRole::Callback).ok_or(InferenceError::InvalidScheme)?.1);
            actual_arguments.push(callable); supplied_slots.push(config.len());
            callback = Some(StageCallback::Protocol { expression: callee, operation: requirement, formal_slot, declaration });
        } else if let Some(callee) = descriptor {
            let (shell, effects, _result, requirement, declaration, result_flow, _kind) = self.source_stage_descriptor(arena, source, identity, callee, item, item_flow, span)?;
            let mut state = self.generic.borrow_mut();
            let reason = state.facts.graph.reason(span, None)?;
            state.facts.graph.unify(protocol_callback.ok_or(InferenceError::InvalidScheme)?, shell, reason)?;
            drop(state);
            actual_arguments.push(shell); supplied_slots.push(config.len());
            callback = Some(StageCallback::Callable { expression: callee, requirement, declaration });
            callback_flow = result_flow; callback_effects = Some(effects);
        } else if let Some(block) = stage.block {
            let kind = if self.in_pure { CallableKind::Pure } else { CallableKind::Proc };
            let effects = if kind == CallableKind::Pure { EffectSummary::Closed(EffectSet::EMPTY) } else { EffectSummary::Variable(self.generic.borrow_mut().facts.graph.fresh_derived_effect_at(level, None)?) };
            let mut parameters = vec![(item, item_flow)];
            if matches!(stage.kind, StreamStageKind::Fold | StreamStageKind::Reduce) {
                let accumulator = supplied.first().copied().flatten().ok_or(InferenceError::Boundary("reduction requires its initial value"))?;
                let flow = configuration.first().and_then(|argument| argument.as_ref()).and_then(|argument| self.stage_argument_flow(arena, identity, argument, span));
                parameters.insert(0, (accumulator, flow));
            }
            let source_parameters = {
                let state = self.generic.borrow();
                parameters.iter().map(|(ty, producer_flow)| {
                    let resolved = state.facts.graph.resolved(*ty)?;
                    // A known dynamic item uses the source dynamic-field boundary;
                    // unresolved items retain their structural graph constraints.
                    let ty = if matches!(state.facts.graph.node(resolved)?, TypeNode::Atom(crate::sema::inference::Atom::Any)) { Type::Any } else { Type::Graph(*ty) };
                    Ok(super::StreamItemFact { ty, producer_flow: *producer_flow })
                }).collect::<Result<Vec<_>, InferenceError>>()?
            };
            let previous_sink = self.stage_callback_effects.replace(effects);
            let result = self.check_stream_block_params_arena(arena, source, block, &source_parameters, matches!(stage.kind, StreamStageKind::Each | StreamStageKind::Tee).then_some(&Type::Unit));
            self.stage_callback_effects = previous_sink;
            // Complete the checked body's calculated permissions before they
            // meet the catalog's caller supplied latent callback binder.
            self.generic.borrow_mut().facts.graph.seal_derived_effects(&[effects])?;
            let result = self.graph_type(&result, span)?;
            let protocol_parameters: Vec<_> = parameters.iter().map(|(ty, _)| *ty).collect();
            let shell = self.stage_callback_shell(kind, &protocol_parameters, result, effects)?;
            let mut state = self.generic.borrow_mut();
            let reason = state.facts.graph.reason(span, None)?;
            state.facts.graph.unify(protocol_callback.ok_or(InferenceError::InvalidScheme)?, shell, reason)?;
            drop(state);
            actual_arguments.push(shell); supplied_slots.push(config.len());
            callback = Some(StageCallback::Block(block)); callback_flow = self.block_producer_flow(arena, block); callback_effects = Some(effects);
        }
        let selected = {
            let mut state = self.generic.borrow_mut();
            let super::generic::GenericState { facts, stage_graph, .. } = &mut *state;
            facts.graph.solve()?;
            facts.graph.candidate_evidence(requirement)?.map(|evidence| stage_graph.metadata(&facts.graph, evidence.candidate).map(|metadata| (metadata.source, metadata.variant))).transpose()?
        };
        let mut roles = Vec::new();
        if !stage.kind.is_adapter() {
            let domain = selected.and_then(|(source, _)| match source { StageSource::Sequence { domain, outer_result } => Some((domain, outer_result)), _ => None });
            roles.extend(self.stage_sequence_effect_bindings(input_flow, 0, domain, span)?);
        }
        if let Some(effects) = callback_effects { roles.push((EffectRole::Callback, effects)); }
        if stage.kind == StreamStageKind::Zip {
            let argument = configuration.first().and_then(|argument| argument.as_ref()).ok_or(InferenceError::Boundary("zip requires its other sequence"))?;
            let flow = self.stage_argument_flow(arena, identity, argument, span);
            secondary_flow = flow;
            let domain = selected.and_then(|(_, variant)| variant.other_sequence.map(|domain| (domain, false)));
            roles.extend(self.stage_sequence_effect_bindings(flow, 1, domain, span)?);
        } else if stage.kind == StreamStageKind::FlatMap {
            let domain = selected.and_then(|(_, variant)| match variant.callback_shape {
                CallbackShape::List => Some((SequenceDomain::List, false)), CallbackShape::Stream => Some((SequenceDomain::Stream, false)),
                CallbackShape::ResultList => Some((SequenceDomain::List, true)), CallbackShape::ResultStream => Some((SequenceDomain::Stream, true)),
                _ => None,
            });
            roles.extend(self.stage_sequence_effect_bindings(callback_flow, 1, domain, span)?);
        }
        {
            let mut state = self.generic.borrow_mut();
            let graph = &mut state.facts.graph;
            if roles.len() != protocol_roles.len() { return Err(InferenceError::InvalidScheme); }
            let reason = graph.reason(span, None)?;
            for (role, actual) in roles {
                let (_, expected) = protocol_roles.iter().find(|(name, _)| *name == role).ok_or(InferenceError::InvalidScheme)?;
                graph.equate_effects(actual, *expected, reason)?;
            }
            graph.solve()?;
            // Fixed caller permissions need not retain variables that exist
            // only inside a completed stage. Pending operation guards keep
            // unresolved producer and callback relationships symbolic.
            let effect_roots: Vec<_> = std::iter::once(effects).chain(outputs.iter().map(|(_, summary)| *summary)).collect();
            graph.seal_derived_effects(&effect_roots)?; graph.solve()?;
        }
        self.check_stage_effects(effects, span);
        let producer_flow = if !outputs.is_empty() {
            let pull = outputs.iter().find(|(role, _)| *role == crate::sema::inference::ProducerRole::Pull).ok_or(InferenceError::InvalidScheme)?.1;
            let close = outputs.iter().find(|(role, _)| *role == crate::sema::inference::ProducerRole::Close).ok_or(InferenceError::InvalidScheme)?.1;
            let alternatives = candidates.into_iter().map(|candidate| super::ProducerFlowOperationAlternative { candidate, path: Some(ProducerPath::default()), transfers: Vec::new(), opaque: false }).collect();
            self.push_source_producer_flow(ProducerFlowSource::Stage(identity), ProducerFlowKind::Operation { requirement, alternatives, outputs: ProducerEffects { pull, close } }, span)
        } else { None };
        let other_item_flow = self.stage_item_transfer(identity, requirement, secondary_flow, StageItemSource::ZipArgument, span)?;
        let callback_item_flow = if stage.kind == StreamStageKind::FlatMap { self.stage_item_transfer(identity, requirement, callback_flow, StageItemSource::CallbackResult, span)? } else { None };
        let data_flow = self.stage_data_flow(identity, &stage.kind, item_flow, other_item_flow, callback_flow, callback_item_flow, span)?;
        let output_flow = match (producer_flow, data_flow) {
            (Some(producer), Some(data)) => self.push_source_producer_flow(ProducerFlowSource::Stage(identity), ProducerFlowKind::Join { inputs: vec![producer, data] }, span),
            (Some(producer), None) => Some(producer),
            (None, Some(data)) => Some(data),
            (None, None) => None,
        };
        let mut state = self.generic.borrow_mut();
        if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.push(requirement); }
        state.facts.stage_operations.insert(identity, SolvedStage { operation: SolvedOperation { requirement, result, effects, receiver: Some(receiver), actual_arguments,
            binding: CallBinding { supplied_slots, default_slots, rest_slot: None, dynamic: None }, caller: self.current_generic, argument_coercions: Vec::new() }, callback, input_producer_flow: input_flow, result_producer_flow: output_flow });
        drop(state);
        Ok((self.graph_view(result), output_flow))
    }

    fn source_stage_error(&mut self, span: Span, error: InferenceError) {
        let code = if let InferenceError::UnsupportedOperation(requirement) = &error {
            let state = self.generic.borrow();
            if let Ok(crate::sema::inference::RequirementTemplate::Operation { family, .. }) = state.facts.graph.requirement_template(*requirement) {
                state.facts.graph.family(family).ok().and_then(|candidates| candidates.first()).and_then(|candidate| state.stage_graph.metadata(&state.facts.graph, *candidate).ok()).and_then(|metadata| match metadata.stage {
                    StreamStageKind::Batch => Some(("check.stream-batch", "batch limits require compatible sequence items")),
                    StreamStageKind::TablePrint => Some(("check.table-print", "table.print requires record items")),
                    StreamStageKind::Sort | StreamStageKind::SortBy => Some(("check.stream-sort", "stream sorting requires sortable items or keys")),
                    _ => None,
                })
            } else if let Ok(crate::sema::inference::RequirementTemplate::Eligibility { predicate, .. }) = state.facts.graph.requirement_template(*requirement) {
                match predicate {
                    crate::sema::inference::Eligibility::Sortable | crate::sema::inference::Eligibility::SortableKey => Some(("check.stream-sort", "stream sorting requires sortable items or keys")),
                    crate::sema::inference::Eligibility::CountKey => Some(("check.stream-count-key", "count keys must be Str, Int, UInt, or Bool")),
                    _ => None,
                }
            } else { None }
        } else { None };
        if let Some((code, message)) = code {
            self.graph_boundary_error(span, message, code);
        } else { self.graph_error(span, error); }
    }

    fn project_stage_flow(&mut self, stage: StageIdentity, input: ProducerFlowId, path: ProducerPath, span: Span) -> Option<ProducerFlowId> {
        self.push_source_producer_flow(ProducerFlowSource::Stage(stage), ProducerFlowKind::Project { input, path }, span)
    }

    fn stage_item_transfer(&mut self, stage: StageIdentity, requirement: crate::sema::inference::RequirementId, input: Option<ProducerFlowId>, source: StageItemSource, span: Span) -> Result<Option<ProducerFlowId>, InferenceError> {
        let Some(input) = input else { return Ok(None); };
        let alternatives = {
            let state = self.generic.borrow();
            let crate::sema::inference::RequirementTemplate::Operation { family, .. } = state.facts.graph.requirement_template(requirement)? else { return Err(InferenceError::InvalidScheme); };
            state.facts.graph.family(family)?.iter().map(|candidate| {
                let metadata = state.stage_graph.metadata(&state.facts.graph, *candidate)?;
                let outer_result = match source {
                    StageItemSource::Input => match metadata.source { StageSource::Sequence { outer_result, .. } => Some(outer_result), StageSource::Str | StageSource::Bytes => None },
                    StageItemSource::ZipArgument => metadata.variant.other_sequence.map(|_| false),
                    StageItemSource::CallbackResult => match metadata.variant.callback_shape {
                        CallbackShape::ResultList | CallbackShape::ResultStream => Some(true),
                        CallbackShape::List | CallbackShape::Stream => Some(false),
                        _ => None,
                    },
                };
                let transfers = outer_result.map(|outer_result| {
                    let mut path = Vec::new();
                    if outer_result { path.push(ProducerPathComponent::ResultSuccess); }
                    path.push(ProducerPathComponent::ListItem);
                    super::ProducerFlowOperationTransfer { input, input_path: ProducerPath(path), output_path: ProducerPath::default() }
                }).into_iter().collect();
                Ok(super::ProducerFlowOperationAlternative { candidate: *candidate, path: None, transfers, opaque: false })
            }).collect::<Result<Vec<_>, InferenceError>>()?
        };
        let empty = EffectSummary::Closed(EffectSet::EMPTY);
        Ok(self.push_source_producer_flow(ProducerFlowSource::Stage(stage), ProducerFlowKind::Operation { requirement, alternatives, outputs: ProducerEffects { pull: empty, close: empty } }, span))
    }

    fn stage_data_flow(&mut self, identity: StageIdentity, stage: &StreamStageKind, item: Option<ProducerFlowId>, other: Option<ProducerFlowId>, callback: Option<ProducerFlowId>, callback_item: Option<ProducerFlowId>, span: Span) -> Result<Option<ProducerFlowId>, InferenceError> {
        use ProducerPathComponent as Path;
        let mut entries = Vec::new();
        let mut add = |path, input| { if let Some(input) = input { entries.push(super::ProducerFlowField { path: ProducerPath(path), input }); } };
        match stage {
            StreamStageKind::Map | StreamStageKind::ParMap => add(vec![Path::ListItem], callback),
            StreamStageKind::Where | StreamStageKind::Sort | StreamStageKind::SortBy | StreamStageKind::Take | StreamStageKind::Drop | StreamStageKind::UniqueBy | StreamStageKind::Repeat | StreamStageKind::Tee | StreamStageKind::Shuffle => add(vec![Path::ListItem], item),
            StreamStageKind::Batch => add(vec![Path::ListItem, Path::ListItem], item),
            StreamStageKind::Enumerate => add(vec![Path::ListItem, Path::RecordField(Name::intern("value"))], item),
            StreamStageKind::Zip => { add(vec![Path::ListItem, Path::RecordField(Name::intern("left"))], item); add(vec![Path::ListItem, Path::RecordField(Name::intern("right"))], other); }
            StreamStageKind::GroupBy => { add(vec![Path::ListItem, Path::RecordField(Name::intern("key"))], callback); add(vec![Path::ListItem, Path::RecordField(Name::intern("items")), Path::ListItem], item); }
            StreamStageKind::Collect => add(vec![Path::ListItem], item),
            StreamStageKind::First | StreamStageKind::Last | StreamStageKind::Min | StreamStageKind::Max => add(vec![Path::ResultSuccess], item),
            StreamStageKind::Fold | StreamStageKind::Reduce => add(Vec::new(), callback),
            StreamStageKind::FlatMap => add(vec![Path::ListItem], callback_item),
            StreamStageKind::ReduceBy => {
                let input = callback.and_then(|callback| self.project_stage_flow(identity, callback, ProducerPath(vec![Path::RecordField(Name::intern("value"))]), span));
                add(vec![Path::MapValue], input);
            }
            StreamStageKind::Each | StreamStageKind::Range | StreamStageKind::Sum | StreamStageKind::Any | StreamStageKind::All | StreamStageKind::Count | StreamStageKind::TablePrint | StreamStageKind::TextStreamLines | StreamStageKind::BytesChunks | StreamStageKind::JsonLines | StreamStageKind::JsonStream => {}
        }
        if entries.is_empty() { return Ok(None); }
        Ok(self.push_source_producer_flow(ProducerFlowSource::Stage(identity), ProducerFlowKind::Aggregate { entries }, span))
    }

    fn stage_callback_shell(&mut self, kind: CallableKind, parameters: &[TypeId], result: TypeId, effects: EffectSummary) -> Result<TypeId, InferenceError> {
        let parameters = parameters.iter().enumerate().map(|(index, ty)| Parameter { label: Name::intern(if parameters.len() == 2 && index == 0 { "<acc>" } else { "<item>" }), ty: *ty, defaulted: false, rest: false }).collect();
        self.generic.borrow_mut().facts.graph.arrow(Arrow { kind, params: parameters, result, effects })
    }

    // A descriptor owns one monotype instance. Candidate protocols invoke this
    // original value; they do not replace its alternatives with a common arrow.
    fn source_stage_protocol_descriptor(&mut self, arena: &ArenaProgram, stage: StageIdentity, expression: ExprId, span: Span) -> Result<(TypeId, Option<super::DeclarationIdentity>), InferenceError> {
        let target = self.graph_callable_target(arena, expression).ok_or(InferenceError::InvalidScheme)?;
        let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
        let callable = {
            let mut state = self.generic.borrow_mut();
            if let Some(scheme) = target.scheme {
                let reason = state.facts.graph.reason(span, None)?;
                let binders = state.facts.graph.scheme_effect_binders(scheme)?;
                let instance = state.facts.graph.instantiate(scheme, level, reason)?;
                let pairs = binders.into_iter().zip(instance.effect_substitutions.iter().copied().map(EffectSummary::Variable)).collect();
                state.producer_inputs.stage_effect_substitutions.insert(stage, pairs);
                state.producer_inputs.stage_requirement_origins.insert(stage, instance.requirement_origins);
                if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.extend(instance.requirements); }
                instance.ty
            } else { target.signature }
        };
        self.record_graph_expression(arena, expression, &Type::Graph(callable));
        self.record_expression_producer_flow(arena, expression, &Type::Graph(callable));
        Ok((callable, target.declaration))
    }

    fn source_stage_descriptor_kind(&mut self, arena: &ArenaProgram, source: &str, expression: ExprId, span: Span) -> Result<CallableKind, InferenceError> {
        self.prepare_graph_callable_value(arena, source, expression);
        if let Some(target) = self.graph_callable_target(arena, expression) {
            let state = self.generic.borrow();
            let graph = &state.facts.graph;
            let signature = graph.callable_signature(target.signature)?;
            let TypeNode::Arrow(arrow) = graph.node(graph.resolved(signature)?)? else { return Err(InferenceError::Boundary("stage callable requires a retained arrow")); };
            if arrow.kind == CallableKind::Stream { return Err(InferenceError::Boundary("stage callable cannot be a stream declaration")); }
            return Ok(arrow.kind);
        }
        let ArenaExprKind::Field { base, name } = arena.arena.expr(expression).kind else { return Err(InferenceError::Boundary("stage descriptor has no registered callable identity")); };
        let ArenaExprKind::Ident(module) = arena.arena.expr(base).kind else { return Err(InferenceError::Boundary("registered stage descriptor needs its module namespace")); };
        let mut state = self.generic.borrow_mut();
        let super::generic::GenericState { facts, registry, .. } = &mut *state;
        let family = registry.module_family(&mut facts.graph, &module.as_str(), &name.as_str(), span)?;
        let mut shape = None;
        for &candidate in facts.graph.family(family)? {
            let metadata = registry.metadata(&facts.graph, candidate)?;
            if metadata.kind == CallableKind::Stream || self.in_pure && metadata.kind != CallableKind::Pure { continue; }
            let current = (metadata.kind, metadata.parameters.len());
            if shape.is_some_and(|shape| shape != current) { return Err(InferenceError::Boundary("stage overloads require a single fixed descriptor shape")); }
            shape = Some(current);
        }
        shape.map(|(kind, _)| kind).ok_or(InferenceError::Boundary("registered stage callback violates its callable domain"))
    }

    fn source_stage_descriptor(&mut self, arena: &ArenaProgram, source: &str, stage: StageIdentity, expression: ExprId, item: TypeId, item_flow: Option<ProducerFlowId>, span: Span) -> Result<(TypeId, EffectSummary, TypeId, crate::sema::inference::RequirementId, Option<super::DeclarationIdentity>, Option<ProducerFlowId>, CallableKind), InferenceError> {
        self.prepare_graph_callable_value(arena, source, expression);
        let Some(target) = self.graph_callable_target(arena, expression) else {
            return self.source_registry_stage_descriptor(arena, stage, expression, item, item_flow, span);
        };
        let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
        let (callable, requirements, origins, pairs) = {
            let mut state = self.generic.borrow_mut();
            if let Some(scheme) = target.scheme {
                let reason = state.facts.graph.reason(span, None)?;
                let binders = state.facts.graph.scheme_effect_binders(scheme)?;
                let instance = state.facts.graph.instantiate(scheme, level, reason)?;
                let pairs = binders.into_iter().zip(instance.effect_substitutions.into_iter().map(EffectSummary::Variable)).collect();
                (instance.ty, instance.requirements, instance.requirement_origins, pairs)
            } else { (target.signature, Vec::new(), Vec::new(), Vec::new()) }
        };
        self.record_graph_expression(arena, expression, &Type::Graph(callable));
        self.record_expression_producer_flow(arena, expression, &Type::Graph(callable));
        let (requirement, result, effects, kind) = {
            let mut state = self.generic.borrow_mut();
            let graph = &mut state.facts.graph;
            let monotype = graph.callable_signature(callable)?;
            let TypeNode::Arrow(arrow) = graph.node(graph.resolved(monotype)?)? else { return Err(InferenceError::Boundary("stage callable requires a retained arrow")); };
            let kind = arrow.kind;
            if kind == CallableKind::Stream { return Err(InferenceError::Boundary("stage callable cannot be a stream declaration")); }
            let result = graph.fresh(level, span)?;
            let effects = EffectSummary::Variable(graph.fresh_derived_effect_at(level, None)?);
            let reason = graph.reason(span, None)?;
            let requirement = graph.require_callable_invocation(InvocationCall { callable, arguments: vec![InvocationArgument { kind: InvocationArgumentKind::Positional, ty: item }], result, effects, domain: if self.in_pure { CallableDomain::Pure } else { CallableDomain::AnyCallable } }, reason)?;
            graph.solve()?;
            graph.seal_derived_effects(&[effects])?; graph.solve()?;
            state.producer_inputs.stage_invocation_requirements.insert(stage, requirement);
            state.producer_inputs.stage_effect_substitutions.insert(stage, pairs);
            state.producer_inputs.stage_requirement_origins.insert(stage, origins);
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.extend(requirements); state.pending.get_mut(&owner).unwrap().requirements.push(requirement); }
            (requirement, result, effects, kind)
        };
        let callee = self.generic.borrow().facts.expression_producer_flows.get(&self.expression_identity(arena, expression)).copied();
        let result_flow = if let (Some(callee), Some(item)) = (callee, item_flow) { self.push_source_producer_flow(ProducerFlowSource::Stage(stage), ProducerFlowKind::StageApply { stage, callee, arguments: vec![item] }, span) } else { None };
        let shell = self.stage_callback_shell(kind, &[item], result, effects)?;
        Ok((shell, effects, result, requirement, target.declaration, result_flow, kind))
    }

    fn source_registry_stage_descriptor(&mut self, arena: &ArenaProgram, stage: StageIdentity, expression: ExprId, item: TypeId, item_flow: Option<ProducerFlowId>, span: Span) -> Result<(TypeId, EffectSummary, TypeId, crate::sema::inference::RequirementId, Option<super::DeclarationIdentity>, Option<ProducerFlowId>, CallableKind), InferenceError> {
        let ArenaExprKind::Field { base, name } = arena.arena.expr(expression).kind else { return Err(InferenceError::Boundary("stage descriptor has no registered callable identity")); };
        let ArenaExprKind::Ident(module) = arena.arena.expr(base).kind else { return Err(InferenceError::Boundary("registered stage descriptor needs its module namespace")); };
        let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
        let (requirement, result, effects, kind, parameter_count) = {
            let mut state = self.generic.borrow_mut();
            let super::generic::GenericState { facts, registry, .. } = &mut *state;
            let family = registry.module_family(&mut facts.graph, &module.as_str(), &name.as_str(), span)?;
            let candidates = facts.graph.family(family)?.to_vec();
            let mut allowed = Vec::new();
            let mut parameter_count = None;
            let mut callable_kind = None;
            for candidate in candidates {
                let metadata = registry.metadata(&facts.graph, candidate)?;
                if self.in_pure && metadata.kind != CallableKind::Pure { continue; }
                if metadata.kind == CallableKind::Stream { continue; }
                if parameter_count.is_some_and(|count| count != metadata.parameters.len()) || callable_kind.is_some_and(|kind| kind != metadata.kind) {
                    return Err(InferenceError::Boundary("stage overloads require a single fixed descriptor shape"));
                }
                parameter_count = Some(metadata.parameters.len()); callable_kind = Some(metadata.kind); allowed.push(candidate);
            }
            let kind = callable_kind.ok_or(InferenceError::Boundary("registered stage callback violates its callable domain"))?;
            let count = parameter_count.ok_or(InferenceError::InvalidScheme)?;
            if count == 0 { return Err(InferenceError::Boundary("stage callback cannot accept its item")); }
            let family = facts.graph.register_family(&allowed)?;
            let outputs = registry.output_effect_bindings(&mut facts.graph, family, level)?;
            let result = facts.graph.fresh(level, span)?;
            let effects = EffectSummary::Variable(facts.graph.fresh_derived_effect_at(level, None)?);
            let reason = facts.graph.reason(span, None)?;
            let mut arguments = vec![None; count]; arguments[0] = Some(item);
            let requirement = facts.graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments, result, effects, effect_bindings: Vec::new(), output_effect_bindings: outputs.clone() }, reason)?;
            facts.graph.solve()?;
            (requirement, result, effects, kind, count)
        };
        let mut formal_arguments = vec![None; parameter_count]; formal_arguments[0] = item_flow;
        let flow = Some(self.registry_operation_producer_flow_from_flows(requirement, None, &formal_arguments, ProducerFlowSource::Stage(stage), span)?);
        if let Some(owner) = self.current_generic { self.generic.borrow_mut().pending.get_mut(&owner).unwrap().requirements.push(requirement); }
        let shell = self.stage_callback_shell(kind, &[item], result, effects)?;
        Ok((shell, effects, result, requirement, None, flow, kind))
    }

    fn stage_argument_flow(&mut self, arena: &ArenaProgram, stage: StageIdentity, argument: &ExpandedArgument, span: Span) -> Option<ProducerFlowId> {
        let (expression, path) = match argument.value {
            ArgumentValueSource::Expression(expression) => (expression, None),
            ArgumentValueSource::RecordField { record, field } => (record, Some(ProducerPath(vec![ProducerPathComponent::RecordField(field)]))),
            ArgumentValueSource::PositionalSplice(expression) => (expression, Some(ProducerPath(vec![ProducerPathComponent::ListItem]))),
        };
        let input = self.generic.borrow().facts.expression_producer_flows.get(&self.expression_identity(arena, expression)).copied()?;
        match path { None => Some(input), Some(path) => self.push_source_producer_flow(ProducerFlowSource::Stage(stage), ProducerFlowKind::Project { input, path }, span) }
    }

    fn stage_sequence_effect_bindings(&mut self, flow: Option<ProducerFlowId>, source: u32, selected: Option<(SequenceDomain, bool)>, span: Span) -> Result<Vec<(EffectRole, EffectSummary)>, InferenceError> {
        let locations = sequence_effect_roles(source);
        if let Some(flow) = flow && let Some((pull, close)) = self.stage_owned_output_effects(flow)? {
            if selected.is_some_and(|domain| domain != (SequenceDomain::Stream, false)) { return Err(InferenceError::InvalidScheme); }
            let empty = EffectSummary::Closed(EffectSet::EMPTY);
            return Ok(vec![(locations[0], pull), (locations[1], close), (locations[2], empty), (locations[3], empty)]);
        }
        let mut roles = Vec::with_capacity(locations.len());
        for projected in [false, true] {
            let needed = selected.map(|(domain, outer_result)| domain == SequenceDomain::Stream && outer_result == projected).unwrap_or(true);
            let permissions = if needed {
                let flow = flow.ok_or(InferenceError::Boundary("stage producer has no source-owned value flow"))?;
                let path = ProducerPath(if projected { vec![ProducerPathComponent::ResultSuccess] } else { Vec::new() });
                self.producer_effects_for_flow(flow, &path, span).ok_or(InferenceError::Boundary("stage producer permissions are not established"))?
            } else { ProducerEffects { pull: EffectSummary::Closed(EffectSet::EMPTY), close: EffectSummary::Closed(EffectSet::EMPTY) } };
            let offset = if projected { 2 } else { 0 };
            roles.extend([(locations[offset], permissions.pull), (locations[offset + 1], permissions.close)]);
        }
        Ok(roles)
    }

    fn stage_owned_output_effects(&mut self, flow: ProducerFlowId) -> Result<Option<(EffectSummary, EffectSummary)>, InferenceError> {
        let mut state = self.generic.borrow_mut();
        let ProducerFlowSource::Stage(identity) = state.facts.producer_flows.node(flow)?.source else { return Ok(None); };
        let Some(stage) = state.facts.stage_operations.get(&identity) else { return Ok(None); };
        // Item and callback recipes can share the stage's source identity.
        // Only the exact published result carries its established root stream.
        if stage.result_producer_flow != Some(flow) { return Ok(None); }
        let requirement = stage.operation.requirement;
        let crate::sema::inference::RequirementTemplate::Operation { family, .. } = state.facts.graph.requirement_template(requirement)? else { return Err(InferenceError::InvalidScheme); };
        let candidates = state.facts.graph.family(family)?.to_vec();
        state.facts.graph.charge_source_fact_work(candidates.len() as u64 + 1)?;
        for candidate in candidates {
            if terminal(&state.stage_graph.metadata(&state.facts.graph, candidate)?.stage) { return Ok(None); }
        }
        state.stage_graph.output_producer_effects(&state.facts.graph, requirement)
    }

    fn check_stage_effects(&mut self, effects: EffectSummary, span: Span) {
        self.record_graph_effect_summary(effects, span);
        let summary = self.generic.borrow().facts.graph.closed_effect_summary(effects);
        match summary {
            Ok(EffectSummary::Closed(bits)) => for (bit, effect) in [(1, super::Effect::Fs), (2, super::Effect::Net), (4, super::Effect::Process), (8, super::Effect::Env), (16, super::Effect::Time), (32, super::Effect::Error), (64, super::Effect::Io)] {
                if bits.0 & bit != 0 { self.require_effect(effect, span, "stream stage"); }
            },
            Ok(EffectSummary::Unknown) => { self.record_effect_contract(&None, "stream stage"); if self.current_effects.is_some() { self.error(span, "stream stage has unknown producer permissions", "check.effect-violation"); } }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::sema::check::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    // This boundary inspects retained constraints, independently of runtime
    // execution, so a source-only fixture cannot certify the graph ownership.
    #[test]
    fn json_line_stage_callbacks_preserve_the_known_dynamic_item_contract() {
        let source = r#"let rows = "{}" |> json.lines |> where .level != "debug" |> group-by f"${.service}:${.level}"
"#;
        let output = checked(source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        output.solved.validate().unwrap();
    }

    #[test]
    fn json_line_dynamic_integer_sum_preserves_the_runtime_item_check() {
        let output = checked("let total = \"{}\" |> json.lines |> map .duration_ms |> sum\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        output.solved.validate().unwrap();
        let state = &output.solved;
        let stage = state.stage_operations.values().find(|stage| {
            state.graph.candidate_evidence(stage.operation.requirement).unwrap().is_some_and(|evidence| {
                matches!(state.operation_catalog.candidate(&state.graph, evidence.candidate).unwrap(), crate::sema::check::SolvedOperationAuthority::Stage(candidate) if candidate.stage == crate::syntax::node::StreamStageKind::Sum)
            })
        });
        let stage = stage.expect("missing selected sum authority");
        let evidence = state.graph.candidate_evidence(stage.operation.requirement).unwrap().unwrap();
        let crate::sema::check::SolvedOperationAuthority::Stage(metadata) = state.operation_catalog.candidate(&state.graph, evidence.candidate).unwrap() else { panic!("sum selected a foreign authority") };
        assert_eq!(metadata.input_numeric, Some(crate::sema::inference::Atom::Any));
        let crate::sema::inference::TypeNode::Arrow(signature) = state.graph.node(state.graph.resolved(evidence.signature).unwrap()).unwrap() else { panic!("sum has no instantiated signature") };
        let crate::sema::inference::TypeNode::Stream(item) = state.graph.node(state.graph.resolved(signature.params[0].ty).unwrap()).unwrap() else { panic!("sum lost its stream input") };
        assert!(matches!(state.graph.node(state.graph.resolved(*item).unwrap()).unwrap(), crate::sema::inference::TypeNode::Atom(crate::sema::inference::Atom::Any)));
        assert!(matches!(state.graph.node(state.graph.resolved(signature.result).unwrap()).unwrap(), crate::sema::inference::TypeNode::Atom(crate::sema::inference::Atom::Int)));
        for input in ["[1.5]", "[\"wrong\"]"] {
            let rejected = checked(&format!("let total = {input} |> sum\n"));
            assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", rejected.diagnostics);
        }
    }

    #[test]
    fn generalized_named_stage_preserves_independent_item_relationships() {
        let source = "pure identity(value) { value }\npure mapped(values) { values |> map(identity) }\nlet integers: List[Int] = mapped([1, 2])\nlet strings: List[Str] = mapped([\"a\", \"b\"])\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn generalized_stage_rejects_incompatible_callback_item() {
        let source = "pure positive(value: Int) -> Bool { value > 0 }\npure selected(values) { values |> where(positive) }\nlet wrong: List[Str] = selected([\"a\"])\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", checked.diagnostics);
    }

    fn checked(source: &str) -> crate::sema::check::CheckOutput {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        Checker::check_arena(&parsed.arena, source)
    }

    #[test]
    fn named_stage_invocation_keeps_declared_labels_and_per_item_defaults() {
        let output = checked("pure add(value: Int, amount: Int = 2) -> Int { value + amount }\nlet values: List[Int] = [1, 2] |> map(add)\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let symbols = output.solved.symbol_owner().clone(); let _symbols = symbols.enter();
        let stage = output.solved.stage_operations.values().next().unwrap();
        let Some(super::StageCallback::Callable { requirement, declaration: Some(_), .. }) = stage.callback else { panic!("named callback must retain its original declaration and invocation"); };
        let invocation = output.solved.graph.invocation_evidence(requirement).unwrap().unwrap();
        let (signature, binding, timing) = invocation.unique_plan().expect("named declaration keeps one original invocation plan");
        assert_eq!(binding.supplied_slots, [0]);
        assert_eq!(binding.default_slots, [1]);
        assert_eq!(timing, crate::sema::inference::InvocationDefaultTiming::AtCall);
        let crate::sema::inference::TypeNode::Arrow(arrow) = output.solved.graph.node(output.solved.graph.resolved(signature).unwrap()).unwrap() else { panic!(); };
        assert_eq!(arrow.params[0].label.as_str().as_str(), "value");
        assert_eq!(arrow.params[1].label.as_str().as_str(), "amount");
        output.solved.validate().unwrap();
    }

    #[test]
    fn stage_block_projection_generalizes_its_original_source_relationship() {
        let output = checked("pure names(values) { values |> map { |item| item.name } }\nlet numbers: List[Int] = names([{name: 7}])\nlet flags: List[Bool] = names([{name: false, extra: \"wide\"}])\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.solved.stage_operations.len(), 1);
        assert!(matches!(output.solved.stage_operations.values().next().unwrap().callback, Some(super::StageCallback::Block(_))));
        output.solved.validate().unwrap();
    }

    #[test]
    fn named_stage_callback_effects_are_consumed_at_materialization() {
        let prefix = "proc clocked(item: Int) [time] -> Int { let _ = time.now(); item }\n";
        let accepted = checked(&format!("{prefix}proc mapped(values: List[Int]) [time] -> List[Int] {{ values |> map(clocked) }}\n"));
        assert!(accepted.diagnostics.is_empty(), "{:?}", accepted.diagnostics);
        accepted.solved.validate().unwrap();
        let rejected = checked(&format!("{prefix}proc mapped(values: List[Int]) [] -> List[Int] {{ values |> map(clocked) }}\n"));
        assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", rejected.diagnostics);
    }

    #[test]
    fn flat_map_propagates_result_failure_without_erasing_its_item_type() {
        let prefix = "pure expanded(item: Str) -> Result[List[Str]] { Ok([item]) }\n";
        let accepted = checked(&format!("{prefix}proc flattened(values: List[Str]) [error] -> List[Str] {{ values |> flat-map(expanded) }}\n"));
        assert!(accepted.diagnostics.is_empty(), "{:?}", accepted.diagnostics);
        accepted.solved.validate().unwrap();
        let rejected = checked(&format!("{prefix}proc flattened(values: List[Str]) [] -> List[Str] {{ values |> flat-map(expanded) }}\n"));
        assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", rejected.diagnostics);
    }

    #[test]
    fn materialized_stage_items_keep_their_dormant_producer_permissions() {
        let prefix = "stream delayed() [time] -> Stream[Int] { let _ = time.now(); yield 7 }\npure identity(value) { value }\npure copied(values) { values |> map(identity) }\nlet rows: List[Stream[Int]] = copied([delayed()])\n";
        let accepted = checked(&format!("{prefix}proc consumed() [time] -> List[Int] {{ rows[0].collect() }}\n"));
        assert!(accepted.diagnostics.is_empty(), "{:?}", accepted.diagnostics);
        accepted.solved.validate().unwrap();
        let rejected = checked(&format!("{prefix}proc consumed() [] -> List[Int] {{ rows[0].collect() }}\n"));
        assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", rejected.diagnostics);
    }

    #[test]
    fn static_reduction_modes_retain_literal_record_spread_values() {
        let output = checked("let options = {min: true}\nlet values: Map[Str] = [\"b\", \"a\"] |> reduce-by(...options) { |value| {key: \"group\", value: value} }\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        output.solved.validate().unwrap();
    }

    #[test]
    fn stage_configuration_binding_keeps_written_argument_order() {
        let output = checked("let rows = [\"a\", \"b\"] |> batch(max_argv: true, count: 2)\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let stage = output.solved.stage_operations.values().next().unwrap();
        assert_eq!(stage.operation.binding.supplied_slots, [2, 0]);
        output.solved.validate().unwrap();
    }

    #[test]
    fn adapters_zip_and_fold_retain_canonical_source_relationships() {
        let output = checked("let lines: List[Str] = \"a\\nb\" |> text.lines\nlet chunks: List[Bytes] = b\"ab\" |> bytes.chunks(1)\nlet pairs = [1] |> zip([\"x\"])\nlet left: Int = pairs[0].left\nlet right: Str = pairs[0].right\nlet total: Int = [1, 2] |> fold(0) { |acc| acc + . }\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.solved.stage_operations.len(), 4);
        output.solved.validate().unwrap();
    }

    #[test]
    fn generalized_zip_preserves_each_input_item_independently() {
        let output = checked("pure pairs(left, right) { left |> zip(right) }\nlet rows = pairs([1], [\"x\"])\nlet number: Int = rows[0].left\nlet text: Str = rows[0].right\nlet flags = pairs([true], [2])\nlet flag: Bool = flags[0].left\nlet other: Int = flags[0].right\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        output.solved.validate().unwrap();
    }

    #[test]
    fn registered_named_callback_keeps_its_operation_certificate() {
        let output = checked("let encoded: List[Result[Str]] = [1, 2] |> map(json.encode)\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let stage = output.solved.stage_operations.values().next().unwrap();
        let Some(super::StageCallback::Callable { requirement, declaration: None, .. }) = stage.callback else { panic!("registered callback must retain its operation certificate"); };
        let symbols = output.solved.symbol_owner().clone(); let _symbols = symbols.enter();
        assert!(output.solved.graph.candidate_evidence(requirement).unwrap().is_some());
        output.solved.validate().unwrap();
    }

    #[test]
    fn generalized_stages_keep_root_and_result_producer_permissions_separate() {
        use crate::sema::inference::{EffectSet, EffectSummary, TypeNode};
        let declarations = "stream delayed() [time, env] -> Stream[Int] { defer { let _ = env.get(\"SETTING\") }; let _ = time.now(); yield 1 }\nproc materialized(values) { values |> collect }\nproc forwarded(values) { materialized(values) }\n";
        for reverse in [false, true] {
            let mut callers = ["proc plain() [time, env] -> Unit { let _ = forwarded(delayed()) }\n", "proc wrapped() [time, env, error] -> Unit { let _ = forwarded(Ok(delayed())) }\n", "proc lists() [] -> Unit { let _ = forwarded([1]) }\n"].to_vec();
            if reverse { callers.reverse(); }
            let source = format!("{declarations}{}", callers.concat());
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let output = Checker::check_arena(&parsed.arena, &source);
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
            let forwarded = output.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == "forwarded").copied().unwrap();
            let mut effects = output.solved.calls.values().filter(|call| call.declaration == Some(forwarded)).map(|call| {
                let TypeNode::Arrow(arrow) = output.solved.graph.node(call.signature).unwrap() else { panic!() };
                output.solved.graph.closed_effect_summary(arrow.effects).unwrap()
            }).collect::<Vec<_>>();
            effects.sort_by_key(|summary| match summary { EffectSummary::Closed(bits) => bits.0, _ => 255 });
            assert_eq!(effects, vec![EffectSummary::Closed(EffectSet::EMPTY), EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0)), EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0 | EffectSet::ERROR.0))]);
            drop(parsed);
            output.solved.validate().unwrap();
        }
        for (caller, effect) in [("proc denied() [time, error] -> Unit { let _ = forwarded(Ok(delayed())) }\n", "env"), ("proc denied() [env, error] -> Unit { let _ = forwarded(Ok(delayed())) }\n", "time"), ("proc denied() [time, env] -> Unit { let _ = forwarded(Ok(delayed())) }\n", "error")] {
            let output = checked(&format!("{declarations}{caller}"));
            assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{effect}: {:?}", output.diagnostics);
        }
    }

    #[test]
    fn typed_stage_domains_demand_only_their_canonical_producer_location() {
        let source = "proc plain(values: Stream[Int]) [time, env] { values |> collect }\nproc wrapped(values: Result[Stream[Int]]) [time, env, error] { values |> collect }\nproc lists(values: List[Int]) [] { values |> collect }\nproc result_lists(values: Result[List[Int]]) [error] { values |> collect }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let output = Checker::check_arena(&parsed.arena, source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let symbols = output.solved.symbol_owner().clone(); let _symbols = symbols.enter();
        for (identity, declaration) in &output.solved.declarations {
            let name = parsed.arena.arena.function_def(identity.declaration).name;
            let locations = declaration.parameter_producers[0].keys().cloned().collect::<Vec<_>>();
            let expected = match name.as_str().as_str() {
                "plain" => vec![super::ProducerPath::default()],
                "wrapped" => vec![super::ProducerPath(vec![super::ProducerPathComponent::ResultSuccess])],
                "lists" | "result_lists" => Vec::new(),
                name => panic!("unexpected declaration {name}"),
            };
            assert_eq!(locations, expected);
        }
        drop(parsed);
        output.solved.validate().unwrap();
    }

    #[test]
    fn generalized_result_sequence_keeps_nested_item_producer_permissions() {
        let prefix = "stream delayed() [time] -> Stream[Int] { let _ = time.now(); yield 7 }\nproc copied(values) { values |> collect }\nlet rows: List[Stream[Int]] = copied(Ok([delayed()]))\n";
        let accepted = checked(&format!("{prefix}proc consumed() [time] -> List[Int] {{ rows[0].collect() }}\n"));
        assert!(accepted.diagnostics.is_empty(), "{:?}", accepted.diagnostics);
        let path = super::ProducerPath(vec![super::ProducerPathComponent::ListItem]);
        assert!(accepted.solved.binding_producers.values().any(|profile| profile.get(&path).is_some_and(|effects| accepted.solved.graph.closed_effect_summary(effects.pull).unwrap() == crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::TIME))));
        accepted.solved.validate().unwrap();
        let rejected = checked(&format!("{prefix}proc consumed() [] -> List[Int] {{ rows[0].collect() }}\n"));
        assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", rejected.diagnostics);
    }

    fn result_sequence_callback_keeps_item_permissions(stage: &str) {
            let prefix = format!("stream delayed() [time] -> Stream[Int] {{ let _ = time.now(); yield 7 }}\npure identity(value) {{ value }}\nproc copied(values) {{ values |> {stage} }}\nlet rows: List[Stream[Int]] = copied(Ok([delayed()]))\n");
            let accepted = checked(&format!("{prefix}proc consumed() [time] -> List[Int] {{ rows[0].collect() }}\n"));
            assert!(accepted.diagnostics.is_empty(), "{stage}: {:?}", accepted.diagnostics);
            accepted.solved.validate().unwrap();
            let rejected = checked(&format!("{prefix}proc consumed() [] -> List[Int] {{ rows[0].collect() }}\n"));
            assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{stage}: {:?}", rejected.diagnostics);
    }

    #[test]
    fn generalized_result_sequence_named_callback_keeps_nested_item_permissions() { result_sequence_callback_keeps_item_permissions("map(identity)"); }

    #[test]
    fn generalized_result_sequence_block_callback_keeps_nested_item_permissions() { result_sequence_callback_keeps_item_permissions("map { . }"); }

    #[test]
    fn chained_stages_keep_pending_item_data_separate_from_their_root_permissions() {
        let prefix = "stream delayed() [time] -> Stream[Int] { let _ = time.now(); yield 7 }\npure identity(value) { value }\npure included(value) -> Bool { true }\nproc copied(values) { values |> map(identity) |> where(included) }\nlet rows: List[Stream[Int]] = copied(Ok([delayed()]))\n";
        let accepted = checked(&format!("{prefix}proc consumed() [time] -> List[Int] {{ rows[0].collect() }}\n"));
        assert!(accepted.diagnostics.is_empty(), "{:?}", accepted.diagnostics);
        accepted.solved.validate().unwrap();
        let rejected = checked(&format!("{prefix}proc consumed() [] -> List[Int] {{ rows[0].collect() }}\n"));
        assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", rejected.diagnostics);
    }

    #[test]
    fn terminal_result_stage_preserves_its_projected_stream_permissions() {
        let prefix = "stream delayed() [time] -> Stream[Int] { let _ = time.now(); yield 7 }\n";
        let accepted = checked(&format!("{prefix}proc consumed() [time, error] -> List[Int] {{ [delayed()] |> first |> collect }}\n"));
        assert!(accepted.diagnostics.is_empty(), "{:?}", accepted.diagnostics);
        accepted.solved.validate().unwrap();
        let rejected = checked(&format!("{prefix}proc consumed() [error] -> List[Int] {{ [delayed()] |> first |> collect }}\n"));
        assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", rejected.diagnostics);
    }

    #[test]
    fn sort_stage_keys_keep_their_eligibility_diagnostic_after_callback_checking() {
        for key in ["[row.name]", "{scores: [row.name]}"] {
            let output = checked(&format!("[{{name: \"a\"}}] |> sort-by {{ |row| {key} }}\n"));
            assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.stream-sort")), "{:?}", output.diagnostics);
        }
    }

    struct SourceStageCase {
        kind: crate::syntax::node::StreamStageKind,
        item: &'static str,
        item_type: &'static str,
        input: &'static str,
        configuration: &'static str,
        block: Option<&'static str>,
    }

    impl SourceStageCase {
        fn suffix(&self, extra: Option<&str>) -> String {
            let configuration = match extra {
                Some(extra) if !self.configuration.is_empty() => format!("{}, {extra}", self.configuration),
                Some(extra) => extra.to_owned(),
                None => self.configuration.to_owned(),
            };
            format!("{}({configuration}){}", self.kind.as_str(), self.block.map(|body| format!(" {{ {body} }}")).unwrap_or_default())
        }

        fn source_domain(&self, stream: bool, outer_result: bool) -> crate::sema::stage_graph::StageSource {
            use crate::sema::stage_graph::{SequenceDomain, StageSource};
            match self.kind {
                crate::syntax::node::StreamStageKind::BytesChunks => StageSource::Bytes,
                crate::syntax::node::StreamStageKind::TextStreamLines | crate::syntax::node::StreamStageKind::JsonLines | crate::syntax::node::StreamStageKind::JsonStream => StageSource::Str,
                _ => StageSource::Sequence { domain: if stream { SequenceDomain::Stream } else { SequenceDomain::List }, outer_result },
            }
        }
    }

    // Each entry supplies actual syntax and a ground item chosen for that
    // contract. The checked source, rather than catalog enumeration, decides
    // whether its original callback and configuration are accepted.
    fn source_stage_cases() -> Vec<SourceStageCase> {
        use crate::syntax::node::StreamStageKind::*;
        let ordinary = [
            (Where, "", Some("|item| true")),
            (Map, "", Some("|item| item")),
            (ParMap, "jobs: 1", Some("|item| item")),
            (Each, "", Some("|item| let _ = item")),
            (Batch, "count: 1", None),
            (Sort, "desc: true", None),
            (SortBy, "desc: true", Some("|item| item")),
            (Take, "1", None),
            (Drop, "1", None),
            (First, "", None),
            (Last, "", None),
            (UniqueBy, "", Some("|item| item")),
            (Enumerate, "", None),
            (Zip, "[\"other\"]", None),
            (Range, "0, 2", None),
            (Repeat, "2", None),
            (Tee, "", Some("|item| let _ = item")),
            (Sum, "", None),
            (Min, "", None),
            (Max, "", None),
            (GroupBy, "", Some("|item| item")),
            (Fold, "0", Some("|acc, item| acc")),
            (Reduce, "0", Some("|acc, item| acc")),
            (FlatMap, "", Some("|item| [item]")),
            (Any, "", Some("|item| true")),
            (All, "", Some("|item| true")),
            (Shuffle, "7", None),
            (Count, "", None),
            (Collect, "", None),
            (ReduceBy, "sum: true", Some("|item| {key: \"group\", value: item}")),
        ];
        let mut cases = ordinary.into_iter().map(|(kind, configuration, block)| SourceStageCase { kind, item: "1", item_type: "Int", input: "[1]", configuration, block }).collect::<Vec<_>>();
        cases.extend([
            SourceStageCase { kind: TablePrint, item: "{name: \"row\"}", item_type: "Row", input: "[{name: \"row\"}]", configuration: "columns: [\"name\"]", block: None },
            SourceStageCase { kind: TextStreamLines, item: "", item_type: "", input: "\"a\\nb\"", configuration: "", block: None },
            SourceStageCase { kind: BytesChunks, item: "", item_type: "", input: "b\"ab\"", configuration: "1", block: None },
            SourceStageCase { kind: JsonLines, item: "", item_type: "", input: "\"1\\n2\"", configuration: "", block: None },
            SourceStageCase { kind: JsonStream, item: "", item_type: "", input: "\"1 2\"", configuration: "", block: None },
        ]);
        cases
    }

    fn assert_source_stage_selection(source: &str, case: &SourceStageCase, expected: crate::sema::stage_graph::StageSource) -> Result<Vec<crate::sema::stage_graph::StageCandidate>, String> {
        use crate::sema::check::SolvedOperationAuthority;
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        if !parsed.diagnostics.is_empty() { return Err(format!("parse: {:?}", parsed.diagnostics)); }
        let output = Checker::check_arena(&parsed.arena, source);
        if !output.diagnostics.is_empty() { return Err(format!("check: {:?}", output.diagnostics)); }
        let symbols = output.solved.symbol_owner().clone(); let _symbols = symbols.enter();
        let (identity, stage) = output.solved.stage_operations.iter().next().ok_or("missing source stage")?;
        if output.solved.stage_operations.len() != 1 { return Err("fixture must retain one source stage".into()); }
        let super::super::ExpressionIdentity { expression, .. } = identity.pipeline;
        let crate::syntax::arena::ArenaExprKind::StructuredPipeline { stages, .. } = parsed.arena.arena.expr(expression).kind else { return Err("stage lost its pipeline expression".into()); };
        if identity.index != stages.start || stages.len != 1 || parsed.arena.arena.stream_stages(stages)[0].kind != case.kind { return Err("stage lost its actual arena identity".into()); }
        let syntax = &parsed.arena.arena.stream_stages(stages)[0];
        match (&stage.callback, syntax.block) {
            (Some(super::StageCallback::Block(actual)), Some(expected)) if *actual == expected => {},
            (Some(super::StageCallback::Callable { expression, .. }), None) if parsed.arena.arena.call_args(syntax.args).iter().any(|argument| matches!(argument.kind, crate::syntax::arena::ArenaCallArgKind::Positional(value) | crate::syntax::arena::ArenaCallArgKind::Named { value, .. } if value == *expression)) => {},
            (None, None) if case.block.is_none() => {},
            _ => return Err("stage callback lost its original source identity".into()),
        }
        let mut requirements = vec![stage.operation.requirement];
        for call in output.solved.calls.values() { requirements.extend_from_slice(&call.requirements); }
        let mut selections = Vec::new();
        for requirement in requirements {
            if output.solved.graph.requirement_origin(requirement).map_err(|error| format!("{error:?}"))? != stage.operation.requirement { continue; }
            let Some(evidence) = output.solved.graph.candidate_evidence(requirement).map_err(|error| format!("{error:?}"))? else { continue; };
            let SolvedOperationAuthority::Stage(metadata) = output.solved.operation_catalog.candidate(&output.solved.graph, evidence.candidate).map_err(|error| format!("{error:?}"))? else { return Err("source stage selected foreign authority".into()); };
            if metadata.stage != case.kind || metadata.source != expected { return Err(format!("wrong selected authority: {:?} {:?}, expected {:?} {:?}", metadata.stage, metadata.source, case.kind, expected)); }
            if metadata.parameters != xsh_registry::stream_parameters::stage_parameters(case.kind.as_str()) || metadata.form.callback_kind.is_some() != case.block.is_some() { return Err("selected source configuration or callback form was lost".into()); }
            selections.push(metadata.clone());
        }
        if selections.is_empty() { return Err("no selected source or instantiated stage certificate".into()); }
        drop(parsed);
        output.solved.validate().map_err(|error| format!("freeze: {error:?}"))?;
        Ok(selections)
    }

    #[test]
    fn source_stage_families_retain_selected_authority_and_reject_extra_configuration() {
        let cases = source_stage_cases();
        let names = cases.iter().map(|case| case.kind.as_str()).collect::<std::collections::BTreeSet<_>>();
        assert_eq!(names.len(), 35);
        let mut failures = Vec::new();
        for case in cases {
            let source = format!("proc observed() [error, io] {{ let _ = {} |> {} }}\n", case.input, case.suffix(None));
            if let Err(error) = assert_source_stage_selection(&source, &case, case.source_domain(false, false)) { failures.push(format!("{} positive: {error}", case.kind.as_str())); }
            let source = format!("proc observed() [error, io] {{ let _ = {} |> {} }}\n", case.input, case.suffix(Some("unexpected: 1")));
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{}: {:?}", case.kind.as_str(), parsed.diagnostics);
            let rejected = Checker::check_arena(&parsed.arena, &source);
            if !rejected.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code.as_deref(), Some("check.arity" | "check.named-arg"))) { failures.push(format!("{} extra configuration accepted: {:?}", case.kind.as_str(), rejected.diagnostics)); }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn generalized_forwarded_stage_families_select_each_actual_sequence_carrier() {
        let mut failures = Vec::new();
        for case in source_stage_cases() {
            if case.kind.is_adapter() {
                let source = format!("proc transformed(values) {{ values |> {} }}\nproc forwarded(values) {{ transformed(values) }}\nproc observed() [error] {{ let _ = forwarded({}) }}\n", case.suffix(None), case.input);
                if let Err(error) = assert_source_stage_selection(&source, &case, case.source_domain(false, false)) { failures.push(format!("{} forwarded scalar: {error}", case.kind.as_str())); }
                continue;
            }
            for stream in [false, true] { for outer_result in [false, true] {
                let argument = if stream { "provided()" } else { case.input };
                let argument = if outer_result { format!("Ok({argument})") } else { argument.to_owned() };
                let source = format!("type Row = {{name: Str}}\nstream provided() [] -> Stream[{}] {{ yield {} }}\nproc transformed(values) {{ values |> {} }}\nproc forwarded(values) {{ transformed(values) }}\nproc observed() [error, io] {{ let _ = forwarded({argument}) }}\n", case.item_type, case.item, case.suffix(None));
                if let Err(error) = assert_source_stage_selection(&source, &case, case.source_domain(stream, outer_result)) { failures.push(format!("{} stream={stream} result={outer_result}: {error}", case.kind.as_str())); }
            } }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn source_stage_callback_and_configuration_variants_retain_exact_metadata() {
        use crate::sema::stage_graph::{CallbackShape, ReductionModes, ReductionShape, SequenceDomain};
        use crate::syntax::node::StreamStageKind::*;
        let cases = [
            (Each, "", "|item| Ok()", "[1]", CallbackShape::Unit, None, None),
            (Tee, "", "|item| Ok()", "[1]", CallbackShape::Unit, None, None),
            (FlatMap, "", "|item| [item]", "[1]", CallbackShape::List, None, None),
            (FlatMap, "", "|item| provided()", "[1]", CallbackShape::Stream, None, None),
            (FlatMap, "", "|item| Ok([item])", "[1]", CallbackShape::ResultList, None, None),
            (FlatMap, "", "|item| Ok(provided())", "[1]", CallbackShape::ResultStream, None, None),
            (Count, "", "|item| item", "[1]", CallbackShape::Value, None, None),
            (ReduceBy, "min: true", "|item| {key: \"group\", value: item}", "[1]", CallbackShape::Value, Some(ReductionModes::MIN), Some(ReductionShape::Value)),
            (ReduceBy, "max: true", "|item| {key: \"group\", value: item}", "[1]", CallbackShape::Value, Some(ReductionModes::MAX), Some(ReductionShape::Value)),
            (ReduceBy, "sum: true", "|item| {key: \"group\", value: item}", "[1]", CallbackShape::Value, Some(ReductionModes::SUM), Some(ReductionShape::Int)),
            (ReduceBy, "sum: true", "|item| {key: \"group\", value: item}", "values", CallbackShape::Value, Some(ReductionModes::SUM), Some(ReductionShape::UInt)),
            (ReduceBy, "sum: true", "|item| {key: \"group\", value: 1.5}", "[1]", CallbackShape::Value, Some(ReductionModes::SUM), Some(ReductionShape::Float)),
            (ReduceBy, "sum: true", "|item| {key: \"group\", value: {left: item, right: item}}", "[1]", CallbackShape::Value, Some(ReductionModes::SUM), Some(ReductionShape::Record)),
            (ReduceBy, "sum: yes(), min: no(), max: no()", "|item| {key: \"group\", value: item}", "[1]", CallbackShape::Value, Some(ReductionModes::ALL), Some(ReductionShape::Int)),
        ];
        let mut failures = Vec::new();
        for (kind, configuration, block, input, shape, modes, reduction) in cases {
            let case = SourceStageCase { kind, item: "1", item_type: "Int", input, configuration, block: Some(block) };
            let parameters = if input == "values" { "values: List[UInt]" } else { "" };
            let source = format!("pure yes() -> Bool {{ true }}\npure no() -> Bool {{ false }}\nstream provided() [] -> Stream[Int] {{ yield 1 }}\nproc observed({parameters}) [error] {{ let _ = {input} |> {} }}\n", case.suffix(None));
            match assert_source_stage_selection(&source, &case, case.source_domain(false, false)) {
                Ok(selections) => for metadata in selections {
                    assert_eq!(metadata.variant.callback_shape, shape, "{source}");
                    assert_eq!(metadata.form.reduction_modes, modes, "{source}");
                    if let Some(reduction) = reduction { assert_eq!(metadata.variant.reduction_shape, reduction, "{source}"); }
                },
                Err(error) => failures.push(format!("{} {block}: {error}", case.kind.as_str())),
            }
        }
        for (kind, input, configuration, argv, other, numeric) in [
            (Batch, "[\"item\"]", "count: 1, max_bytes: 32, max_argv: true", true, None, None),
            (Zip, "[1]", "provided()", false, Some(SequenceDomain::Stream), None),
            (Sum, "values", "", false, None, Some(crate::sema::inference::Atom::UInt)),
        ] {
            let case = SourceStageCase { kind, item: "1", item_type: "Int", input, configuration, block: None };
            let parameters = if input == "values" { "values: List[UInt]" } else { "" };
            let source = format!("stream provided() [] -> Stream[Str] {{ yield \"other\" }}\nproc observed({parameters}) [error] {{ let _ = {input} |> {} }}\n", case.suffix(None));
            match assert_source_stage_selection(&source, &case, case.source_domain(false, false)) {
                Ok(selections) => for metadata in selections {
                    assert_eq!(metadata.form.batch_argv, argv, "{source}");
                    assert_eq!(metadata.variant.other_sequence, other, "{source}");
                    assert_eq!(metadata.input_numeric, numeric, "{source}");
                },
                Err(error) => failures.push(format!("{} {configuration}: {error}", case.kind.as_str())),
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn source_named_stage_callbacks_keep_callable_domain_and_reject_wrong_arity() {
        use crate::sema::inference::CallableKind;
        let mut failures = Vec::new();
        for case in source_stage_cases().into_iter().filter(|case| xsh_registry::stream_parameters::stage_accepts_callable(case.kind.as_str())) {
            let body = case.block.unwrap().strip_prefix("|item| ").unwrap();
            for kind in [CallableKind::Pure, CallableKind::Proc] {
                let (definition, effects, clock) = if kind == CallableKind::Pure { ("pure", "", "") } else { ("proc", "[time]", "let _ = time.now(); ") };
                let source = format!("{definition} callback(item: Int) {effects} {{ {clock}{body} }}\nproc observed() [time, error] {{ let _ = [1] |> {}(block: callback{}) }}\n", case.kind.as_str(), if case.configuration.is_empty() { String::new() } else { format!(", {}", case.configuration) });
                match assert_source_stage_selection(&source, &case, case.source_domain(false, false)) {
                    Ok(selections) => for metadata in selections { assert_eq!(metadata.form.callback_kind, Some(kind), "{source}"); },
                    Err(error) => failures.push(format!("{} {kind:?}: {error}", case.kind.as_str())),
                }
                let source = format!("{definition} callback(item: Int, required: Int) {effects} {{ {clock}{body} }}\nproc observed() [time, error] {{ let _ = [1] |> {}(block: callback{}) }}\n", case.kind.as_str(), if case.configuration.is_empty() { String::new() } else { format!(", {}", case.configuration) });
                let output = checked(&source);
                if !output.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.arity")) { failures.push(format!("{} {kind:?} wrong arity: {:?}", case.kind.as_str(), output.diagnostics)); }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn forwarded_stage_families_keep_input_pull_and_cleanup_permissions() {
        let mut failures = Vec::new();
        for case in source_stage_cases().into_iter().filter(|case| !case.kind.is_adapter()) {
            let prefix = format!("type Row = {{name: Str}}\nstream provided() [time, env] -> Stream[{}] {{ defer {{ let _ = env.get(\"SETTING\") }}; let _ = time.now(); yield {} }}\nproc transformed(values) {{ values |> {} }}\nproc forwarded(values) {{ transformed(values) }}\n", case.item_type, case.item, case.suffix(None));
            let accepted = format!("{prefix}proc observed() [time, env, error, io] {{ let _ = forwarded(Ok(provided())) }}\n");
            if let Err(error) = assert_source_stage_selection(&accepted, &case, case.source_domain(true, true)) { failures.push(format!("{} permission positive: {error}", case.kind.as_str())); }
            let omissions = if case.kind == crate::syntax::node::StreamStageKind::TablePrint {
                vec![("env, error, io", "time"), ("time, env, io", "error")]
            } else {
                vec![("time, error", "env"), ("env, error", "time"), ("time, env", "error")]
            };
            for (permissions, missing) in omissions {
                let rejected = checked(&format!("{prefix}proc observed() [{permissions}] {{ let _ = forwarded(Ok(provided())) }}\n"));
                if !rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")) { failures.push(format!("{} missing {missing}: {:?}", case.kind.as_str(), rejected.diagnostics)); }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn source_inline_stage_callbacks_keep_their_pure_callable_domain() {
        let mut failures = Vec::new();
        for case in source_stage_cases().into_iter().filter(|case| case.block.is_some()) {
            let source = format!("pure observed() {{ {} |> {} }}\n", case.input, case.suffix(None));
            match assert_source_stage_selection(&source, &case, case.source_domain(false, false)) {
                Ok(selections) => for metadata in selections { assert_eq!(metadata.form.callback_kind, Some(crate::sema::inference::CallableKind::Pure), "{source}"); },
                Err(error) => failures.push(format!("{} pure original block: {error}", case.kind.as_str())),
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn source_stage_families_reject_receivers_outside_their_catalog_domain() {
        let mut failures = Vec::new();
        for case in source_stage_cases() {
            let input = match case.kind {
                crate::syntax::node::StreamStageKind::BytesChunks => "\"wrong\"",
                crate::syntax::node::StreamStageKind::TextStreamLines | crate::syntax::node::StreamStageKind::JsonLines | crate::syntax::node::StreamStageKind::JsonStream => "b\"wrong\"",
                _ => "1",
            };
            let output = checked(&format!("proc observed() [error, io] {{ let _ = {input} |> {} }}\n", case.suffix(None)));
            if output.diagnostics.is_empty() { failures.push(case.kind.as_str()); }
        }
        assert!(failures.is_empty(), "accepted wrong receivers: {}", failures.join(", "));
    }

    #[test]
    fn source_stage_callback_domains_and_static_flags_reject_invalid_shapes() {
        let cases = [
            "[1] |> where { |item| 1 }",
            "[1] |> any { |item| 1 }",
            "[1] |> all { |item| 1 }",
            "[1] |> map { |item| let _ = item }",
            "[1] |> par-map(jobs: 1) { |item| let _ = item }",
            "[1] |> flat-map { |item| item }",
            "[1] |> sort-by { |item| [item] }",
            "[[1]] |> sort",
            "[1] |> fold(0) { |acc, item| \"wrong\" }",
            "[1] |> reduce(0) { |acc, item| \"wrong\" }",
            "[1] |> reduce-by(sum: true) { |item| {key: [item], value: item} }",
            "[1] |> count { |item| 1.5 }",
            "[1] |> zip(1)",
            "[{name: \"row\"}] |> batch(count: 1, max_argv: true)",
            "[1] |> batch",
            "[1] |> reduce-by(sum: true, min: true) { |item| {key: item, value: item} }",
            "[1] |> reduce-by(sum: false) { |item| {key: item, value: item} }",
            "[1] |> take(-1)",
            "[1] |> drop(-1)",
            "[1] |> repeat(-1)",
            "[1] |> range(1)",
            "[1] |> par-map(jobs: 0) { |item| item }",
            "b\"ab\" |> bytes.chunks(0)",
            "[{name: \"row\"}] |> table.print(columns: [1])",
        ];
        let mut failures = Vec::new();
        for source in cases {
            let output = checked(&format!("proc observed() [error, io] {{ let _ = {source} }}\n"));
            if output.diagnostics.is_empty() { failures.push(source); }
        }
        assert!(failures.is_empty(), "accepted invalid stage contracts: {}", failures.join("\n"));
    }

    #[test]
    fn source_unit_stage_descriptors_reject_value_returning_callables() {
        for stage in ["each", "tee"] {
            let output = checked(&format!("pure value(item: Int) -> Int {{ item }}\nproc observed() [] {{ let _ = [1] |> {stage}(value) }}\n"));
            assert!(output.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code.as_deref(), Some("check.type-mismatch" | "check.type-relationship"))), "{stage}: {:?}", output.diagnostics);
        }
    }

    #[test]
    fn source_count_keys_keep_their_existing_eligibility_diagnostic() {
        for key in ["Ok(item)", "1.5", "[item]"] {
            let output = checked(&format!("[1] |> count {{ |item| {key} }}\n"));
            assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.stream-count-key")), "{key}: {:?}", output.diagnostics);
        }
    }

    #[test]
    fn forwarded_flat_map_callback_carriers_keep_projected_pull_and_cleanup_roles() {
        use crate::sema::stage_graph::CallbackShape;
        let mut failures = Vec::new();
        for (body, shape, permissions, omissions) in [
            ("|item| delayed()", CallbackShape::Stream, "time, env", vec![("time", "env"), ("env", "time")]),
            ("|item| Ok(delayed())", CallbackShape::ResultStream, "time, env, error", vec![("time, error", "env"), ("env, error", "time"), ("time, env", "error")]),
        ] {
            let case = SourceStageCase { kind: crate::syntax::node::StreamStageKind::FlatMap, item: "1", item_type: "Int", input: "[1]", configuration: "", block: Some(body) };
            let prefix = format!("stream delayed() [time, env] -> Stream[Int] {{ defer {{ let _ = env.get(\"SETTING\") }}; let _ = time.now(); yield 1 }}\nproc transformed(values) {{ values |> {} }}\nproc forwarded(values) {{ transformed(values) }}\n", case.suffix(None));
            let source = format!("{prefix}proc observed() [{permissions}] {{ let _ = forwarded([1]) }}\n");
            match assert_source_stage_selection(&source, &case, case.source_domain(false, false)) {
                Ok(selections) => for metadata in selections { assert_eq!(metadata.variant.callback_shape, shape); },
                Err(error) => failures.push(format!("{shape:?}: {error}")),
            }
            for (permissions, missing) in omissions {
                let output = checked(&format!("{prefix}proc observed() [{permissions}] {{ let _ = forwarded([1]) }}\n"));
                if !output.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")) { failures.push(format!("{shape:?} missing {missing}: {:?}", output.diagnostics)); }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn generalized_flat_map_list_callbacks_preserve_dormant_item_permissions() {
        for body in ["|item| [item]", "|item| Ok([item])"] {
            let prefix = format!("stream delayed() [time] -> Stream[Int] {{ let _ = time.now(); yield 7 }}\nproc transformed(values) {{ values |> flat-map {{ {body} }} }}\nproc forwarded(values) {{ transformed(values) }}\nlet rows: List[Stream[Int]] = forwarded(Ok([delayed()]))\n");
            let accepted = checked(&format!("{prefix}proc consumed() [time] -> List[Int] {{ rows[0].collect() }}\n"));
            assert!(accepted.diagnostics.is_empty(), "{body}: {:?}", accepted.diagnostics);
            accepted.solved.validate().unwrap();
            let rejected = checked(&format!("{prefix}proc consumed() [] -> List[Int] {{ rows[0].collect() }}\n"));
            assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{body}: {:?}", rejected.diagnostics);
        }
    }

    #[test]
    fn collect_source_forms_keep_configuration_arity_diagnostics() {
        for stage in ["collect(1)", "collect(jobs: 1)", "collect { . }"] {
            let output = checked(&format!("let _ = [1] |> {stage}\n"));
            assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.arity")), "{stage}: {:?}", output.diagnostics);
        }
    }

    #[test]
    fn table_sink_retains_only_executed_source_permissions() {
        let output = checked("proc displayed() [] { let columns = [\"name\"]; [{name: \"row\"}] |> table.print(columns:) }\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        output.solved.validate().unwrap();
        assert_eq!(output.solved.declarations.values().next().unwrap().required_effects,
            crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY));

        let prefix = "type Row = {name: Str}\nstream rows() [time] -> Stream[Row] { let _ = time.now(); yield {name: \"row\"} }\n";
        let output = checked(&format!("{prefix}proc displayed() [time] {{ rows() |> table.print(columns: [\"name\"]) }}\n"));
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        output.solved.validate().unwrap();
        let output = checked(&format!("{prefix}proc displayed() [] {{ rows() |> table.print(columns: [\"name\"]) }}\n"));
        assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")),
            "{:?}", output.diagnostics);
    }

}
