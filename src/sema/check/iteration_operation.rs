use super::{ComprehensionIdentity, SolvedComprehensionClause, BindingIdentity, CallBinding, Checker, ProducerEffects, ProducerFlowId, ProducerFlowKind, ProducerFlowSource, ProducerPath, ProducerPathComponent, SolvedOperation, StatementIdentity, Type};
use crate::sema::inference::{EffectProjection, EffectRole, EffectSet, EffectSummary, InferenceError, OperationCall};
use crate::sema::operation_graph::{IterableDomain, PreparedLanguageOperation};
use crate::source::Span;
use crate::syntax::arena::{ArenaBindingTargetKind, ArenaProgram, BindingTargetId, ExprId, StmtId};

#[derive(Clone, Copy)]
enum IterationSyntax { For, ListComprehension, MapComprehension, YieldDelegation }

fn iteration_roles() -> [EffectRole; 4] {
    [EffectRole::Pull { source: 0 }, EffectRole::Close { source: 0 }, EffectRole::PullProjection { source: 0, projection: EffectProjection::ResultSuccess }, EffectRole::CloseProjection { source: 0, projection: EffectProjection::ResultSuccess }]
}

impl Checker {
    pub(super) fn check_graph_iteration_operation(&mut self, arena: &ArenaProgram, statement: StmtId, expression: ExprId, operand: &Type) -> (Type, Option<PreparedLanguageOperation>, Option<ProducerFlowId>) {
        let identity = StatementIdentity { source: arena.arena.stmt(statement).span.source_id, namespace: self.current_namespace, statement };
        self.check_source_iteration_operation(arena, ProducerFlowSource::Statement(identity), expression, operand, IterationSyntax::For)
    }

    pub(super) fn check_graph_comprehension_operation(&mut self, arena: &ArenaProgram, identity: ComprehensionIdentity, expression: ExprId, operand: &Type, map: bool) -> (Type, Option<PreparedLanguageOperation>, Option<ProducerFlowId>) {
        self.check_source_iteration_operation(arena, ProducerFlowSource::Comprehension(identity), expression, operand, if map { IterationSyntax::MapComprehension } else { IterationSyntax::ListComprehension })
    }

    pub(super) fn check_graph_delegation_operation(&mut self, arena: &ArenaProgram, statement: StmtId, expression: ExprId, operand: &Type) -> (Type, Option<PreparedLanguageOperation>, Option<ProducerFlowId>) {
        let identity = StatementIdentity { source: arena.arena.stmt(statement).span.source_id, namespace: self.current_namespace, statement };
        self.check_source_iteration_operation(arena, ProducerFlowSource::Statement(identity), expression, operand, IterationSyntax::YieldDelegation)
    }

    fn check_source_iteration_operation(&mut self, arena: &ArenaProgram, identity: ProducerFlowSource, expression: ExprId, operand: &Type, syntax: IterationSyntax) -> (Type, Option<PreparedLanguageOperation>, Option<ProducerFlowId>) {
        let span = arena.arena.expr(expression).span;
        let delegation = matches!(syntax, IterationSyntax::YieldDelegation);
        let existing = {
            let state = self.generic.borrow();
            match identity {
                ProducerFlowSource::Statement(identity) => state.facts.statement_operations.get(&identity).map(|operation| (operation.clone(), state.facts.statement_producer_flows.get(&identity).copied())),
                ProducerFlowSource::Comprehension(identity) => state.facts.comprehension_operations.get(&identity).map(|clause| (clause.operation.clone(), Some(clause.item_producer_flow))),
                _ => None,
            }
        };
        if !self.graph_generation || existing.is_some() {
            let Some((fact, flow)) = existing else { return (Type::Invalid, None, None); };
            let state = self.generic.borrow();
            let operation = state.facts.graph.candidate_evidence(fact.requirement).ok().flatten().and_then(|evidence| state.language_operations.metadata(&state.facts.graph, evidence.candidate).ok()).map(|metadata| metadata.operation);
            if !delegation && self.retry_attempt_depth > 0
                && let Ok(crate::sema::inference::RequirementTemplate::Operation { call, .. }) = state.facts.graph.requirement_template(fact.requirement)
                && let Ok(call) = state.facts.graph.operation_call(call)
                && let Some(bound) = call.declared_error_bound {
                let error = self.graph_view(bound);
                if let Some(errors) = self.error_boundary_errors.last_mut() { errors.push((error, span)); }
            }
            return (self.graph_view(fact.result), operation, flow);
        }
        let outcome = (|| {
            // The reached expression owns its checked receiver type. Reusing
            // that endpoint also preserves composite shells around binders.
            let retained = self.generic.borrow().facts.expressions.get(&self.expression_identity(arena, expression)).copied();
            let receiver = match retained { Some(receiver) => receiver, None => self.graph_type(operand, span)? };
            let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
            let fixed_completion = !delegation && self.retry_attempt_depth == 0 && self.current_generic.is_some_and(|owner| {
                let definition = arena.arena.function_def(owner.declaration);
                !definition.return_ty_defaulted || definition.test_declaration || self.generic.borrow().pending[&owner].fixed_return
            });
            let mut declared_error_bound = if fixed_completion {
                match self.current_return.clone() {
                    Some(Type::Result(_, error)) => Some(self.graph_type(&error, span)?),
                    _ => None,
                }
            } else { None };
            let (family, selected) = {
                let mut state = self.generic.borrow_mut();
                let super::generic::GenericState { facts, language_operations, .. } = &mut *state;
                let family = if delegation { language_operations.delegation_family(&mut facts.graph, span)? } else { language_operations.iteration_family(&mut facts.graph, span)? };
                // Probe the canonical receiver relationship before asking a
                // source value for permissions at candidate-specific paths.
                let selected = facts.graph.trial(|graph| {
                    let result = graph.fresh(level, span)?;
                    let effects = EffectSummary::Variable(graph.fresh_derived_effect_at(level, None)?);
                    let roles = iteration_roles().into_iter().map(|role| Ok((role, EffectSummary::Variable(graph.fresh_effect_at(level, None)?)))).collect::<Result<Vec<_>, InferenceError>>()?;
                    let reason = graph.reason(span, None)?;
                    let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(receiver)], result, effects, effect_bindings: roles, output_effect_bindings: Vec::new() }, reason)?;
                    graph.solve()?;
                    graph.candidate_evidence(requirement)?.map(|evidence| language_operations.metadata(graph, evidence.candidate).map(|metadata| metadata.operation.clone())).transpose()
                })?;
                (family, selected)
            };
            if !delegation && (selected.is_none() || matches!(selected, Some(PreparedLanguageOperation::Iteration { domain: IterableDomain::Map | IterableDomain::Str | IterableDomain::Bytes, outer_result: true }))) {
                self.record_error_boundary_producer_input(arena, expression);
            }
            let local_completion = !delegation && selected.is_none() && self.retry_attempt_depth > 0;
            if local_completion {
                let port = self.generic.borrow_mut().facts.graph.fresh(level, span)?;
                let error = self.graph_view(port);
                self.error_boundary_errors.last_mut().ok_or(InferenceError::InvalidScheme)?.push((error, span));
                declared_error_bound = Some(port);
            }
            let family = if selected.is_none() && !delegation && !fixed_completion && !local_completion {
                    // An unresolved source cannot select a different enclosing
                    // Result wrapper for each call of the same checked body.
                    let mut state = self.generic.borrow_mut();
                    let super::generic::GenericState { facts, language_operations, .. } = &mut *state;
                    language_operations.unresolved_iteration_family(&mut facts.graph, span)?
            } else { family };
            let mut roles = Vec::new();
            for projected in [false, true] {
                let needed = match &selected {
                    Some(PreparedLanguageOperation::Iteration { domain, outer_result }) => *domain == IterableDomain::Stream && *outer_result == projected,
                    None => true,
                    _ => return Err(InferenceError::InvalidScheme),
                };
                let permissions = if needed {
                    let path = ProducerPath(if projected { vec![ProducerPathComponent::ResultSuccess] } else { Vec::new() });
                    self.producer_effects_for_expression(arena, expression, &path, span).unwrap_or(ProducerEffects { pull: EffectSummary::Unknown, close: EffectSummary::Unknown })
                } else { ProducerEffects { pull: EffectSummary::Closed(EffectSet::EMPTY), close: EffectSummary::Closed(EffectSet::EMPTY) } };
                let pair = if projected { [iteration_roles()[2], iteration_roles()[3]] } else { [iteration_roles()[0], iteration_roles()[1]] };
                roles.extend([(pair[0], permissions.pull), (pair[1], permissions.close)]);
            }
            let (requirement, result, effects, candidates) = {
                let mut state = self.generic.borrow_mut();
                let graph = &mut state.facts.graph;
                let result = graph.fresh(level, span)?;
                // The calculated summary shares its declaration's lifetime
                // with the producer ports it may later quantify.
                let effects = EffectSummary::Variable(graph.fresh_derived_effect_at(level, None)?);
                let reason = graph.reason(span, None)?;
                let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound, receiver: None, arguments: vec![Some(receiver)], result, effects, effect_bindings: roles, output_effect_bindings: Vec::new() }, reason)?;
                graph.solve()?;
                graph.seal_derived_effects(&[effects])?;
                graph.solve()?;
                let candidates = graph.family(family)?.to_vec();
                if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).ok_or(InferenceError::InvalidScheme)?.requirements.push(requirement); }
                if let ProducerFlowSource::Statement(identity) = identity {
                    state.facts.statement_operations.insert(identity, SolvedOperation { requirement, result, effects, receiver: None, actual_arguments: vec![receiver], argument_coercions: Vec::new(), binding: CallBinding { dynamic: None, supplied_slots: vec![0], default_slots: Vec::new(), rest_slot: None }, caller: self.current_generic });
                }
                (requirement, result, effects, candidates)
            };
            self.check_iteration_effects(effects, span);
            let input = self.generic.borrow().facts.expression_producer_flows.get(&self.expression_identity(arena, expression)).copied();
            let flow = if let Some(input) = input {
                let alternatives = {
                    let state = self.generic.borrow();
                    candidates.into_iter().map(|candidate| {
                        let PreparedLanguageOperation::Iteration { domain, outer_result } = state.language_operations.metadata(&state.facts.graph, candidate)?.operation else { return Err(InferenceError::InvalidScheme); };
                        let mut path = if outer_result { vec![ProducerPathComponent::ResultSuccess] } else { Vec::new() };
                        let transfers = match domain {
                            IterableDomain::List | IterableDomain::Stream => { path.push(ProducerPathComponent::ListItem); vec![super::producer::ProducerFlowOperationTransfer { input, input_path: ProducerPath(path), output_path: ProducerPath::default() }] },
                            IterableDomain::Map => { path.push(ProducerPathComponent::MapValue); vec![super::producer::ProducerFlowOperationTransfer { input, input_path: ProducerPath(path), output_path: ProducerPath(vec![ProducerPathComponent::RecordField(crate::symbol::Name::intern("value"))]) }] },
                            IterableDomain::Str | IterableDomain::Bytes => Vec::new(),
                        };
                        Ok(super::producer::ProducerFlowOperationAlternative { candidate, path: None, transfers, opaque: false })
                    }).collect::<Result<Vec<_>, InferenceError>>()?
                };
                self.push_source_producer_flow(identity, ProducerFlowKind::Operation { requirement, alternatives, outputs: ProducerEffects { pull: EffectSummary::Closed(EffectSet::EMPTY), close: EffectSummary::Closed(EffectSet::EMPTY) } }, span)
            } else { None };
            match identity {
                ProducerFlowSource::Statement(identity) => if let Some(flow) = flow { self.generic.borrow_mut().facts.statement_producer_flows.insert(identity, flow); },
                ProducerFlowSource::Comprehension(identity) => {
                    let input_producer_flow = input.ok_or(InferenceError::Boundary("comprehension source has no retained producer flow"))?;
                    let item_producer_flow = flow.ok_or(InferenceError::Boundary("comprehension item has no retained producer flow"))?;
                    self.generic.borrow_mut().facts.comprehension_operations.insert(identity, SolvedComprehensionClause {
                        operation: SolvedOperation { requirement, result, effects, receiver: None, actual_arguments: vec![receiver], argument_coercions: Vec::new(), binding: CallBinding { dynamic: None, supplied_slots: vec![0], default_slots: Vec::new(), rest_slot: None }, caller: self.current_generic },
                        input_producer_flow, item_producer_flow,
                    });
                }
                _ => return Err(InferenceError::InvalidScheme),
            }
            Ok((self.graph_view(result), selected, flow))
        })();
        match outcome {
            Ok(value) => value,
            Err(InferenceError::UnsupportedOperation(_)) => {
                let (message, code) = match syntax {
                    IterationSyntax::For => ("`for` iterates over List, Stream, Map, Str, or Bytes values", "check.for-iterator"),
                    IterationSyntax::ListComprehension => ("comprehension iterates over List, Stream, Map, Str, or Bytes values", "check.listcomp-iterator"),
                    IterationSyntax::MapComprehension => ("comprehension iterates over List, Stream, Map, Str, or Bytes values", "check.mapcomp-iterator"),
                    IterationSyntax::YieldDelegation => ("yield delegation requires a List or Stream; handle Results explicitly", "check.yield-delegation"),
                };
                self.error(span, message, code);
                (Type::Invalid, None, None)
            }
            Err(error) => { self.graph_error(span, error); (Type::Invalid, None, None) }
        }
    }

    pub(super) fn record_delegation_close_permissions(&mut self, arena: &ArenaProgram, statement: StmtId, span: Span) {
        if !self.graph_generation { return; }
        let Some(owner) = self.current_generic else { return; };
        let identity = StatementIdentity { source: arena.arena.stmt(statement).span.source_id, namespace: self.current_namespace, statement };
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let Some(parent) = state.pending[&owner].producer_effects else { return Ok(()); };
            let operation = state.facts.statement_operations.get(&identity).ok_or(InferenceError::InvalidScheme)?;
            let crate::sema::inference::RequirementTemplate::Operation { call, .. } = state.facts.graph.requirement_template(operation.requirement)? else { return Err(InferenceError::InvalidScheme); };
            let close = state.facts.graph.operation_call(call)?.effect_bindings.iter().find(|(role, _)| *role == EffectRole::Close { source: 0 }).ok_or(InferenceError::InvalidScheme)?.1;
            let reason = state.facts.graph.reason(span, None)?;
            state.facts.graph.include_effects(close, parent.close, reason)
        })();
        if let Err(error) = outcome { self.graph_error(span, error); }
    }

    fn check_iteration_effects(&mut self, effects: EffectSummary, span: Span) {
        self.record_graph_effect_summary(effects, span);
        let summary = self.generic.borrow().facts.graph.closed_effect_summary(effects);
        match summary {
            Ok(EffectSummary::Closed(bits)) => for (bit, effect) in [(1, super::Effect::Fs), (2, super::Effect::Net), (4, super::Effect::Process), (8, super::Effect::Env), (16, super::Effect::Time), (32, super::Effect::Error), (64, super::Effect::Io)] {
                if bits.0 & bit != 0 { self.require_effect(effect, span, "iteration"); }
            },
            Ok(EffectSummary::Unknown) => { self.record_effect_contract(&None, "iteration"); if self.current_effects.is_some() { self.error(span, "iteration has unknown producer permissions", "check.effect-violation"); } },
            _ => {}
        }
    }

    pub(super) fn check_graph_map_key_eligibility(&mut self, key: &Type, span: Span) {
        let Type::Graph(key) = key else { return; };
        if !self.graph_generation { return; }
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let reason = state.facts.graph.reason(span, None)?;
            let requirement = state.facts.graph.require_eligibility(crate::sema::inference::Eligibility::MapKey, *key, reason)?;
            state.facts.graph.solve()?;
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).ok_or(InferenceError::InvalidScheme)?.requirements.push(requirement); }
            Ok(())
        })();
        if let Err(error) = outcome { self.graph_error(span, error); }
    }

    pub(super) fn iteration_binding_type(&mut self, arena: &ArenaProgram, target: BindingTargetId, ty: &Type, span: Span) -> Type {
        let ArenaBindingTargetKind::Record { fields, .. } = arena.arena.binding_target(target).kind else { return ty.clone(); };
        if matches!(ty, Type::Any | Type::Unknown | Type::Invalid) { return ty.clone(); }
        let mut record = std::collections::BTreeMap::new();
        for field in arena.arena.destructure_fields(fields) {
            let field_span = arena.arena.span(field.span);
            let outcome = if self.graph_generation {
                (|| {
                    let receiver = self.graph_type(ty, span)?;
                    let mut state = self.generic.borrow_mut();
                    let reason = state.facts.graph.reason(field_span, None)?;
                    let level = if self.current_generic.is_some() { 1 } else { 0 };
                    state.facts.graph.require_field(receiver, field.name, level, reason)
                })()
            } else {
                let identity = BindingIdentity { source: field_span.source_id, namespace: self.current_namespace, target: field.target };
                self.generic.borrow().facts.bindings.get(&identity).map(|binding| binding.ty).ok_or(InferenceError::InvalidScheme)
            };
            let field_ty = match outcome {
                Ok(ty) => self.graph_view(ty),
                Err(InferenceError::MissingField(_)) => { self.error(field_span, "unknown destructured field", "check.destructure-field"); Type::Invalid },
                Err(error) => { self.graph_error(field_span, error); Type::Invalid }
            };
            let field_ty = self.iteration_binding_type(arena, field.target, &field_ty, field_span);
            record.insert(field.name, field_ty);
        }
        Type::Record(record)
    }

    pub(super) fn record_iteration_binding_flow(&mut self, arena: &ArenaProgram, target: BindingTargetId, ty: &Type, flow: Option<ProducerFlowId>, span: Span) {
        self.record_graph_binding(target, ty, false, span);
        if !self.graph_generation { return; }
        let name = match arena.arena.binding_target(target).kind {
            ArenaBindingTargetKind::Name(name) => name,
            ArenaBindingTargetKind::Record { fields, .. } => {
                let Type::Record(types) = ty else { return; };
                for field in arena.arena.destructure_fields(fields) {
                    let Some(ty) = types.get(&field.name) else { continue; };
                    let field_span = arena.arena.span(field.span);
                    let identity = BindingIdentity { source: field_span.source_id, namespace: self.current_namespace, target: field.target };
                    let projected = flow.and_then(|input| self.push_source_producer_flow(ProducerFlowSource::Binding { identity, version: 0 }, ProducerFlowKind::Project { input, path: ProducerPath(vec![ProducerPathComponent::RecordField(field.name)]) }, field_span));
                    self.record_iteration_binding_flow(arena, field.target, ty, projected, field_span);
                }
                return;
            }
        };
        if name == "_" { return; }
        let Some(input) = flow else { return; };
        let identity = BindingIdentity { source: span.source_id, namespace: self.current_namespace, target };
        if !self.generic.borrow().facts.bindings.contains_key(&identity) { return; }
        let Some(flow) = self.push_source_producer_flow(ProducerFlowSource::Binding { identity, version: 0 }, ProducerFlowKind::Join { inputs: vec![input] }, span) else { return; };
        self.generic.borrow_mut().facts.binding_producer_flows.insert((identity, 0), flow);
        if let Some(binding) = self.current_scope_mut().get_mut(&name) { binding.producer_flow = Some(flow); binding.producer_binding = Some((identity, 0)); }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Checker, ReturnElaboration};
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn generalized_iteration_forwards_each_canonical_receiver_domain() {
        let declarations = "proc visit(values) { for item in values { let _ = item }; 1 }\nproc forwarded(values) { visit(values) }\nstream rows() [] -> Stream[Int] { yield 7 }\n";
        let calls = [
            "let listed: Int = forwarded([7])\n",
            "let streamed: Int = forwarded(rows())\n",
            "let mapped: Int = forwarded({[\"name\"]: 7})\n",
            "let changed_map: Int = forwarded({[7]: \"seven\"})\n",
            "let textual: Int = forwarded(\"word\")\n",
            "let binary: Int = forwarded(b\"word\")\n",
        ];
        for reverse in [false, true] {
            let mut calls = calls.to_vec();
            if reverse { calls.reverse(); }
            let source = format!("{declarations}{}", calls.concat());
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert_eq!(checked.solved.statement_operations.len(), 1);
            let (identity, operation) = checked.solved.statement_operations.iter().next().unwrap();
            assert_eq!(checked.solved.statement_owners.get(identity).copied(), operation.caller);
            assert_eq!(operation.binding.supplied_slots, vec![0]);
            assert_eq!(operation.actual_arguments.len(), 1);
            assert!(checked.solved.declarations.values().filter(|declaration| declaration.kind == crate::sema::inference::CallableKind::Proc).all(|declaration| !checked.solved.graph.scheme(declaration.scheme).unwrap().requirements.is_empty()));
            drop(parsed);
            checked.solved.validate().unwrap();
        }
    }

    #[test]
    fn generalized_iteration_keeps_fresh_pull_cleanup_and_projected_permissions() {
        use crate::sema::inference::{EffectSet, EffectSummary, TypeNode};
        let declarations = "stream delayed() [time, env] -> Stream[Int] { defer { let _ = env.get(\"SETTING\") }; let _ = time.now(); yield 1 }\nproc visit(values) { for item in values { let _ = item; break }; 1 }\nproc forwarded(values) { visit(values) }\n";
        for reverse in [false, true] {
            let mut callers = ["proc plain() [time, env] -> Unit { let _ = forwarded(delayed()) }\n", "proc wrapped() [time, env, error] -> Unit { let _ = forwarded(Ok(delayed())) }\n", "proc materialized() [] -> Unit { let _ = forwarded([1]) }\n"].to_vec();
            if reverse { callers.reverse(); }
            let source = format!("{declarations}{}", callers.concat());
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let forwarded = checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == "forwarded").copied().unwrap();
            let mut effects: Vec<_> = checked.solved.calls.values().filter(|call| call.declaration == Some(forwarded)).map(|call| {
                let TypeNode::Arrow(arrow) = checked.solved.graph.node(call.signature).unwrap() else { panic!("the source call retains an arrow") };
                checked.solved.graph.closed_effect_summary(arrow.effects).unwrap()
            }).collect();
            effects.sort_by_key(|summary| match summary { EffectSummary::Closed(bits) => bits.0, _ => 255 });
            assert_eq!(effects, vec![EffectSummary::Closed(EffectSet::EMPTY), EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0)), EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0 | EffectSet::ERROR.0))]);
            let visit = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "visit").unwrap().1;
            assert_eq!(visit.parameter_producers[0].len(), 2);
            assert!(visit.parameter_producers[0].contains_key(&super::ProducerPath::default()));
            assert!(visit.parameter_producers[0].contains_key(&super::ProducerPath(vec![super::ProducerPathComponent::ResultSuccess])));
            drop(parsed);
            checked.solved.validate().unwrap();
        }
        for (source, effect) in [("proc denied() [time] -> Unit { let _ = forwarded(delayed()) }\n", "env"), ("proc denied() [env] -> Unit { let _ = forwarded(delayed()) }\n", "time"), ("proc denied() [time, env] -> Unit { let _ = forwarded(Ok(delayed())) }\n", "error"), ("proc denied() [time, error] -> Unit { let _ = forwarded(Ok(delayed())) }\n", "env"), ("proc denied() [env, error] -> Unit { let _ = forwarded(Ok(delayed())) }\n", "time")] {
            let source = format!("{declarations}{source}");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains(effect)), "{effect}: {:?}", checked.diagnostics);
            assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.message.contains("unknown producer")), "{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn materialized_iteration_keeps_operand_creation_permissions_separate_from_consumption() {
        use crate::frontend::query::SolvedQuery;
        use crate::sema::inference::{EffectSet, EffectSummary, RequirementTemplate};
        use crate::syntax::arena::ArenaStmtKind;
        for (name, ty, value) in [("Bytes", "Bytes", "b\"word\""), ("List", "List[Int]", "[7]"), ("Map", "Map[Int]", "{[\"name\"]: 7}"), ("Str", "Str", "\"word\"")] {
            for wrapped in [false, true] {
                let ty = if wrapped { format!("Result[{ty}]") } else { ty.to_string() };
                let value = if wrapped { format!("Ok({value})") } else { value.to_string() };
                let source = format!("proc make_value() [time, env] -> {ty} {{ let _ = time.now(); let _ = env.get(\"UNREAD\"); {value} }}\nproc visit(values) [error] -> Result[Unit] {{ for item in values {{ let _ = item }} }}\nproc forwarded(values) [error] -> Result[Unit] {{ visit(values) }}\nproc accepted() [time, env, error] -> Result[Unit] {{ forwarded(make_value()) }}\n");
                let parsed = Parser::parse_source_arena_only(SourceId::new(85), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
                let visit = *checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == "visit").unwrap();
                let body = parsed.arena.arena.function_def(visit.declaration).body;
                let clauses = parsed.arena.arena.stmt_ids(parsed.arena.arena.block(body).statements).filter_map(|statement| match parsed.arena.arena.stmt(statement).kind { ArenaStmtKind::For { iter, .. } => Some((statement, iter)), _ => None }).collect::<Vec<_>>();
                assert_eq!(clauses.len(), 1);
                let (statement, iterable) = clauses[0];
                let identity = super::StatementIdentity { source: SourceId::new(85), namespace: visit.namespace, statement };
                let operation = &checked.solved.statement_operations[&identity];
                assert_eq!(operation.caller, Some(visit));
                assert_eq!(checked.solved.statement_owners[&identity], visit);
                let iterable = super::super::ExpressionIdentity { source: identity.source, namespace: identity.namespace, expression: iterable };
                assert_eq!(checked.solved.graph.resolved(operation.actual_arguments[0]).unwrap(), checked.solved.graph.resolved(checked.solved.expressions[&iterable]).unwrap());
                let forwarded = *checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == "forwarded").unwrap();
                let calls = checked.solved.calls.values().filter(|call| call.declaration == Some(forwarded)).collect::<Vec<_>>();
                assert_eq!(calls.len(), 1);
                let selections = calls[0].requirements.iter().filter_map(|requirement| checked.solved.graph.candidate_evidence(*requirement).unwrap().map(|evidence| (*requirement, evidence))).collect::<Vec<_>>();
                assert_eq!(selections.len(), 1);
                let (requirement, evidence) = selections[0];
                let super::super::SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap() else { panic!("materialized iteration has a language-owned domain") };
                assert_eq!(metadata.authority, format!("language.iteration.{name}{}", if wrapped { ".Result" } else { "" }));
                assert_eq!(checked.solved.graph.closed_effect_summary(evidence.effects).unwrap(), EffectSummary::Closed(if wrapped { EffectSet::ERROR } else { EffectSet::EMPTY }));
                let RequirementTemplate::Operation { call, .. } = checked.solved.graph.requirement_template(requirement).unwrap() else { panic!("the selected iteration keeps its call") };
                let call = checked.solved.graph.operation_call(call).unwrap();
                assert_eq!(call.effect_bindings.len(), 4);
                assert!(call.effect_bindings.iter().all(|(_, effect)| checked.solved.graph.closed_effect_summary(*effect).unwrap() == EffectSummary::Closed(EffectSet::EMPTY)), "materialized values have no producer pull or close work");
                drop(parsed);
                checked.solved.validate().unwrap();
                let before = checked.solved.graph.counters().clone();
                let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
                let answer = query.statement_operation(identity).unwrap();
                assert_eq!(answer.actual_arguments.len(), 1);
                assert_eq!(answer.binding.supplied_slots, vec![0]);
                assert_eq!(checked.solved.graph.counters(), &before);
                for (allowed, denied) in [("env, error", "time"), ("time, error", "env")] {
                    let invalid = source.replace("proc accepted() [time, env, error]", &format!("proc denied() [{allowed}]"));
                    let parsed = Parser::parse_source_arena_only(SourceId::new(85), &invalid);
                    let checked = Checker::check_arena(&parsed.arena, &invalid);
                    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains(denied)), "{name}, {denied}: {:?}", checked.diagnostics);
                    assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.message.contains("unknown producer")), "{:?}", checked.diagnostics);
                }
                if wrapped {
                    let invalid = format!("proc denied(values: {ty}) [] -> Result[Unit] {{ for item in values {{ let _ = item }} }}\n");
                    let parsed = Parser::parse_source_arena_only(SourceId::new(85), &invalid);
                    let checked = Checker::check_arena(&parsed.arena, &invalid);
                    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains("error")), "{name}: {:?}", checked.diagnostics);
                }
            }
        }
    }

    #[test]
    fn generic_result_list_iteration_retains_its_source_operation_and_error_permission() {
        use crate::frontend::query::{NormalizedEffect, NormalizedEffectRoleReference, NormalizedProducerFlowKind, NormalizedProducerFlowSource, NormalizedRequirement, SolvedQuery};
        use crate::sema::inference::{EffectSet, EffectSummary, RequirementTemplate, TypeNode};
        use crate::syntax::arena::ArenaStmtKind;
        let declarations = "error SourceFailure = Missing(message: Str)\nproc visit(values) { for item in values { let _ = item }; 1 }\nproc forwarded(values) { visit(values) }\nlet numbers: Result[List[Int], SourceFailure] = Ok([7])\nlet words: Result[List[Str], SourceFailure] = Ok([\"word\"])\n";
        let callers = ["proc numeric() [error] -> Unit { let result: Int = forwarded(numbers) }\n", "proc textual() [error] -> Unit { let result: Int = forwarded(words) }\n"];
        for reverse in [false, true] {
            let mut callers = callers.to_vec();
            if reverse { callers.reverse(); }
            let source = format!("{declarations}{}", callers.concat());
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let visit = *checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == "visit").unwrap();
            let body = parsed.arena.arena.function_def(visit.declaration).body;
            let statements = parsed.arena.arena.stmt_ids(parsed.arena.arena.block(body).statements).filter_map(|statement| match parsed.arena.arena.stmt(statement).kind { ArenaStmtKind::For { iter, .. } => Some((statement, iter)), _ => None }).collect::<Vec<_>>();
            assert_eq!(statements.len(), 1);
            let (statement, iterable) = statements[0];
            let identity = super::StatementIdentity { source: parsed.arena.arena.stmt(statement).span.source_id, namespace: visit.namespace, statement };
            assert_eq!(checked.solved.statement_operations.len(), 1);
            let operation = &checked.solved.statement_operations[&identity];
            assert_eq!(operation.caller, Some(visit));
            assert_eq!(checked.solved.statement_owners[&identity], visit);
            assert_eq!(operation.binding.supplied_slots, vec![0]);
            let iterable = super::super::ExpressionIdentity { source: identity.source, namespace: identity.namespace, expression: iterable };
            assert_eq!(checked.solved.graph.resolved(operation.actual_arguments[0]).unwrap(), checked.solved.graph.resolved(checked.solved.expressions[&iterable]).unwrap());
            let RequirementTemplate::Operation { call, .. } = checked.solved.graph.requirement_template(operation.requirement).unwrap() else { panic!("the original For owns its iterable requirement") };
            let call = checked.solved.graph.operation_call(call).unwrap();
            assert_eq!(call.arguments, vec![Some(operation.actual_arguments[0])]);
            assert_eq!(call.result, operation.result);
            let forwarded = *checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == "forwarded").unwrap();
            let calls = checked.solved.calls.values().filter(|call| call.declaration == Some(forwarded)).collect::<Vec<_>>();
            assert_eq!(calls.len(), 2);
            let mut item_types = Vec::new();
            for call in calls {
                let TypeNode::Arrow(signature) = checked.solved.graph.node(call.signature).unwrap() else { panic!("each forwarded source call retains its signature") };
                assert_eq!(checked.solved.graph.closed_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet::ERROR));
                let selected = call.requirements.iter().filter_map(|requirement| checked.solved.graph.candidate_evidence(*requirement).unwrap()).map(|evidence| checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap()).collect::<Vec<_>>();
                assert_eq!(selected.len(), 1);
                assert!(matches!(selected[0], super::super::SolvedOperationAuthority::Language(metadata) if metadata.authority == "language.iteration.List.Result" && metadata.operation == crate::sema::operation_graph::PreparedLanguageOperation::Iteration { domain: crate::sema::operation_graph::IterableDomain::List, outer_result: true }));
                let TypeNode::Result(list, _) = checked.solved.graph.node(checked.solved.graph.resolved(signature.params[0].ty).unwrap()).unwrap() else { panic!("the actual receiver remains an outer Result") };
                let TypeNode::List(item) = checked.solved.graph.node(*list).unwrap() else { panic!("the actual receiver remains a List") };
                item_types.push(checked.solved.graph.export_type(*item).unwrap().to_string());
            }
            item_types.sort();
            assert_eq!(item_types, vec!["Int", "Str"]);
            drop(parsed);
            checked.solved.validate().unwrap();
            let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
            let before = checked.solved.graph.counters().instantiations;
            let answer = query.statement_operation(identity).unwrap();
            let NormalizedRequirement::Operation { candidates, arguments, .. } = &answer.requirement else { panic!("the cold source fact retains its operation family") };
            assert_eq!(arguments, &answer.actual_arguments.iter().cloned().map(Some).collect::<Vec<_>>());
            let candidate = candidates.iter().find(|candidate| candidate.public_label == "language.iteration.List.Result").unwrap();
            assert_eq!(candidate.effect_roles.len(), 4);
            assert!(candidate.effect_roles.iter().all(|(_, role)| *role == NormalizedEffectRoleReference::Fixed(Vec::new())));
            let flow = query.statement_producer_flow(identity).unwrap();
            let root = &flow.nodes[flow.root as usize];
            assert!(matches!(&root.source, NormalizedProducerFlowSource::Statement(source) if source.statement == identity.statement && source.source == identity.source));
            let NormalizedProducerFlowKind::Operation { alternatives, outputs, .. } = &root.kind else { panic!("the exact For item flow retains its selected transfers") };
            assert!(alternatives.iter().any(|alternative| alternative.public_label == "language.iteration.List.Result" && !alternative.opaque));
            assert_eq!(outputs.pull, NormalizedEffect::Closed(Vec::new()));
            assert_eq!(outputs.close, NormalizedEffect::Closed(Vec::new()));
            assert_eq!(answer.semantic_parity(&answer), Ok(true));
            assert_eq!(flow.semantic_parity(&flow), Ok(true));
            assert_eq!(checked.solved.graph.counters().instantiations, before);
        }
        let denied = format!("{declarations}proc denied() [] -> Unit {{ let result: Int = forwarded(numbers) }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &denied);
        let checked = Checker::check_arena(&parsed.arena, &denied);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains("error")), "{:?}", checked.diagnostics);
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.message.contains("unknown producer")), "{:?}", checked.diagnostics);
    }

    #[test]
    fn iteration_reuses_original_receiver_endpoints_for_statements_and_comprehensions() {
        use crate::frontend::query::{NormalizedRequirement, SolvedQuery};
        use crate::syntax::arena::{ArenaCompQualifier, ArenaExprKind, ArenaStmtKind};
        for (ty, argument, authority, effects) in [
            ("List[Int]", "[7]", "language.iteration.List", "[]"),
            ("Map[Int]", "{[\"name\"]: 7}", "language.iteration.Map", "[]"),
            ("Str", "\"word\"", "language.iteration.Str", "[]"),
            ("Bytes", "b\"word\"", "language.iteration.Bytes", "[]"),
            ("Result[List[Int]]", "Ok([7])", "language.iteration.List.Result", "[error]"),
        ] {
            for body in ["for item in values { let _ = item }; 1", "let _ = [item for item in values]; 1", "let _ = {[\"item\"]: item for item in values}; 1"] {
                let source = format!("proc visit(values: {ty}) {effects} -> Int {{ {body} }}\nlet result: Int = visit({argument})\n");
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
                assert_eq!(checked.solved.statement_operations.len() + checked.solved.comprehension_operations.len(), 1);
                let (operation, iterable) = if let Some((identity, operation)) = checked.solved.statement_operations.iter().next() {
                    let ArenaStmtKind::For { iter, .. } = parsed.arena.arena.stmt(identity.statement).kind else { panic!("the original statement is a For") };
                    (operation, super::super::ExpressionIdentity { source: identity.source, namespace: identity.namespace, expression: iter })
                } else {
                    let (identity, clause) = checked.solved.comprehension_operations.iter().next().unwrap();
                    let qualifiers = match parsed.arena.arena.expr(identity.expression.expression).kind {
                        ArenaExprKind::ListComp { qualifiers, .. } | ArenaExprKind::MapComp { qualifiers, .. } => qualifiers,
                        _ => panic!("the actual parent expression owns the comprehension clause"),
                    };
                    let ArenaCompQualifier::For { iter, .. } = parsed.arena.arena.comp_qualifiers(qualifiers)[identity.qualifier as usize] else { panic!("the actual qualifier is a generator") };
                    (&clause.operation, super::super::ExpressionIdentity { expression: iter, ..identity.expression })
                };
                assert_eq!(operation.actual_arguments.len(), 1);
                assert_eq!(operation.actual_arguments[0], checked.solved.expressions[&iterable], "the canonical operation reuses the original checked expression endpoint");
                let evidence = checked.solved.graph.candidate_evidence(operation.requirement).unwrap().unwrap();
                assert!(matches!(checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap(), super::super::SolvedOperationAuthority::Language(metadata) if metadata.authority == authority));
                let statement = checked.solved.statement_operations.keys().next().copied();
                let comprehension = checked.solved.comprehension_operations.keys().next().copied();
                drop(parsed);
                checked.solved.validate().unwrap();
                let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
                let before = checked.solved.graph.counters().instantiations;
                let answer = match statement {
                    Some(identity) => query.statement_operation(identity).unwrap(),
                    None => query.comprehension_operation(comprehension.unwrap()).unwrap(),
                };
                let NormalizedRequirement::Operation { arguments, candidates, .. } = &answer.requirement else { panic!("the original source fact retains the canonical family") };
                assert_eq!(arguments, &answer.actual_arguments.iter().cloned().map(Some).collect::<Vec<_>>());
                assert!(candidates.iter().any(|candidate| candidate.public_label == authority));
                assert_eq!(answer.semantic_parity(&answer), Ok(true));
                assert_eq!(checked.solved.graph.counters().instantiations, before);
            }
        }
    }

    #[test]
    fn generalized_iteration_direct_stream_permission_control() {
        let source = "stream delayed() [time, env] -> Stream[Int] { defer { let _ = env.get(\"SETTING\") }; let _ = time.now(); yield 1 }\nproc visit(values) { for item in values { let _ = item }; 1 }\nproc plain() [time, env] -> Unit { let _ = visit(delayed()) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn generalized_iteration_map_destructuring_keeps_key_value_relationships() {
        let declaration = "proc visit(values) { for {key, value} in values { let _: Str = key; let _: Int = value }; 1 }\n";
        for (call, valid) in [("visit({[\"name\"]: 7})", true), ("visit([{key: \"name\", value: 7}])", true), ("visit({[7]: 7})", false), ("visit({[\"name\"]: \"seven\"})", false)] {
            let source = format!("{declaration}let count: Int = {call}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            if valid { assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics); drop(parsed); checked.solved.validate().unwrap(); }
            else { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", checked.diagnostics); }
        }
    }

    #[test]
    fn generalized_iteration_item_bindings_retain_nested_producer_profiles() {
        for (parameter, target, argument) in [("List[Stream[Int]]", "rows", "[delayed()]"), ("Map[Str, Stream[Int]]", "{value: rows, ..}", "{[\"name\"]: delayed()}")] {
            let source = format!("stream delayed() [time, env] -> Stream[Int] {{ defer {{ let _ = env.get(\"SETTING\") }}; let _ = time.now(); yield 1 }}\nproc visit(values: {parameter}) {{ for {target} in values {{ for item in rows {{ let _ = item; break }} }} }}\nproc accepted() [time, env] -> Unit {{ let _ = visit({argument}) }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let visit = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "visit").unwrap().1;
            assert_eq!(visit.parameter_producers[0].len(), 1);
            drop(parsed);
            checked.solved.validate().unwrap();
            let rejected = source.replace("proc accepted() [time, env]", "proc denied() [time]");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &rejected);
            let checked = Checker::check_arena(&parsed.arena, &rejected);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn generalized_comprehension_keeps_all_iterable_item_relationships() {
        let declarations = "type Entry = {key: Str, value: Int}\nproc copied(values) { [item for item in values] }\nproc forwarded(values) { copied(values) }\nstream rows() [] -> Stream[Int] { yield 7 }\n";
        let calls = ["let listed: List[Int] = forwarded([7])\n", "let streamed: List[Int] = forwarded(rows())\n", "let mapped: List[Entry] = forwarded({[\"name\"]: 7})\n", "let textual: List[Str] = forwarded(\"word\")\n", "let binary: List[Int] = forwarded(b\"word\")\n"];
        for reverse in [false, true] {
            let mut calls = calls.to_vec(); if reverse { calls.reverse(); }
            let source = format!("{declarations}{}", calls.concat());
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            drop(parsed);
            checked.solved.validate().unwrap();
        }
    }

    #[test]
    fn generalized_comprehension_qualifiers_keep_distinct_source_values() {
        let source = "pure copied(left, right) { [inner for outer in left if true for inner in right] }\nlet texts: List[Str] = copied([1], [\"word\"])\nlet numbers: List[Int] = copied(b\"word\", [7])\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        drop(parsed);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn generalized_yield_delegation_keeps_list_and_stream_item_relationships() {
        use crate::frontend::query::{NormalizedEffectRoleReference, NormalizedProducerFlowKind, NormalizedProducerFlowSource, NormalizedRequirement, SolvedQuery};
        use crate::sema::inference::{EffectSet, EffectSummary, RequirementTemplate, TypeNode};
        use crate::syntax::arena::ArenaStmtKind;
        let declarations = "stream copied(values) { yield @values }\nstream forwarded(values) { yield @copied(values) }\nstream rows() [] -> Stream[Int] { yield 7 }\n";
        for calls in ["let numbers: Stream[Int] = forwarded([7])\nlet words: Stream[Str] = forwarded([\"word\"])\nlet live: Stream[Int] = forwarded(rows())\n", "let live: Stream[Int] = forwarded(rows())\nlet words: Stream[Str] = forwarded([\"word\"])\nlet numbers: Stream[Int] = forwarded([7])\n"] {
            let source = format!("{declarations}{calls}");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let mut identities = Vec::new();
            for name in ["copied", "forwarded"] {
                let caller = *checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == name).unwrap();
                let body = parsed.arena.arena.function_def(caller.declaration).body;
                let statements = parsed.arena.arena.stmt_ids(parsed.arena.arena.block(body).statements).filter_map(|statement| match parsed.arena.arena.stmt(statement).kind { ArenaStmtKind::YieldDelegate(value) => Some((statement, value)), _ => None }).collect::<Vec<_>>();
                assert_eq!(statements.len(), 1, "each actual producer body has one delegated source");
                let (statement, value) = statements[0];
                let identity = super::StatementIdentity { source: parsed.arena.arena.stmt(statement).span.source_id, namespace: caller.namespace, statement };
                let operation = &checked.solved.statement_operations[&identity];
                assert_eq!(operation.caller, Some(caller));
                assert_eq!(checked.solved.statement_owners[&identity], caller);
                assert_eq!(operation.binding.supplied_slots, vec![0]);
                let value = super::super::ExpressionIdentity { source: identity.source, namespace: identity.namespace, expression: value };
                assert_eq!(checked.solved.graph.resolved(operation.actual_arguments[0]).unwrap(), checked.solved.graph.resolved(checked.solved.expressions[&value]).unwrap());
                let RequirementTemplate::Operation { call, .. } = checked.solved.graph.requirement_template(operation.requirement).unwrap() else { panic!("the actual delegated statement retains its operation") };
                let call = checked.solved.graph.operation_call(call).unwrap();
                assert_eq!(call.arguments, vec![Some(operation.actual_arguments[0])]);
                assert_eq!(call.result, operation.result);
                identities.push(identity);
            }
            assert_eq!(checked.solved.statement_operations.len(), identities.len());
            let forwarded = *checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == "forwarded").unwrap();
            let calls = checked.solved.calls.values().filter(|call| call.declaration == Some(forwarded)).collect::<Vec<_>>();
            assert_eq!(calls.len(), 3);
            for call in calls {
                let TypeNode::Arrow(signature) = checked.solved.graph.node(call.signature).unwrap() else { panic!("each source call retains its producer signature") };
                assert_eq!(checked.solved.graph.closed_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet::EMPTY));
                let selected = call.requirements.iter().filter_map(|requirement| checked.solved.graph.candidate_evidence(*requirement).unwrap()).map(|evidence| match checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap() {
                    super::super::SolvedOperationAuthority::Language(metadata) => metadata.authority,
                    _ => panic!("delegation selects a language-owned iterable contract"),
                }).collect::<std::collections::BTreeSet<_>>();
                let expected: std::collections::BTreeSet<_> = match checked.solved.graph.node(checked.solved.graph.resolved(signature.params[0].ty).unwrap()).unwrap() {
                    TypeNode::List(_) => ["language.yield_delegation.List", "language.yield_delegation.Stream"].into_iter().collect(),
                    TypeNode::Stream(_) => ["language.yield_delegation.Stream"].into_iter().collect(),
                    _ => panic!("the two forwarded source domains retain List or Stream"),
                };
                assert_eq!(selected, expected);
            }
            drop(parsed);
            checked.solved.validate().unwrap();
            let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
            let before = checked.solved.graph.counters().instantiations;
            for identity in identities {
                let answer = query.statement_operation(identity).unwrap();
                let NormalizedRequirement::Operation { candidates, arguments, declared_error_bound, .. } = &answer.requirement else { panic!("the cold delegation fact retains its operation family") };
                assert_eq!(arguments, &answer.actual_arguments.iter().cloned().map(Some).collect::<Vec<_>>());
                assert_eq!(declared_error_bound, &None);
                assert_eq!(candidates.iter().map(|candidate| candidate.public_label.as_str()).collect::<std::collections::BTreeSet<_>>(), ["language.yield_delegation.List", "language.yield_delegation.Stream"].into_iter().collect::<std::collections::BTreeSet<_>>());
                let listed = candidates.iter().find(|candidate| candidate.public_label == "language.yield_delegation.List").unwrap();
                assert_eq!(listed.effect_roles.len(), 4);
                assert!(listed.effect_roles.iter().all(|(_, role)| *role == NormalizedEffectRoleReference::Fixed(Vec::new())));
                let flow = query.statement_producer_flow(identity).unwrap();
                let root = &flow.nodes[flow.root as usize];
                assert!(matches!(&root.source, NormalizedProducerFlowSource::Statement(source) if source.statement == identity.statement && source.source == identity.source));
                let NormalizedProducerFlowKind::Operation { alternatives, .. } = &root.kind else { panic!("delegated item flow retains its canonical transfers") };
                assert_eq!(alternatives.len(), 2);
                assert!(alternatives.iter().all(|alternative| !alternative.opaque));
                assert_eq!(answer.semantic_parity(&answer), Ok(true));
                assert_eq!(flow.semantic_parity(&flow), Ok(true));
            }
            assert_eq!(checked.solved.graph.counters().instantiations, before);
        }
        for invalid in ["false", "\"word\"", "b\"word\"", "{[\"name\"]: 7}", "Ok([7])"] {
            let source = format!("{declarations}let invalid = copied({invalid})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn comprehension_outputs_retain_nested_producer_permissions() {
        let producer = "stream delayed() [time, env] -> Stream[Int] { defer { let _ = env.get(\"SETTING\") }; let _ = time.now(); yield 1 }\n";
        for declarations in [
            "proc copied(values) { [item for item in values] }\nlet rows = copied([delayed()])\nproc consume() [time, env] -> Unit { for row in rows { for item in row { let _ = item; break } } }\n",
            "proc copied(values) { {key: value for {key, value} in values} }\nlet rows = copied({[\"name\"]: delayed()})\nproc consume() [time, env] -> Unit { for {value: row} in rows { for item in row { let _ = item; break } } }\n",
        ] {
            let source = format!("{producer}{declarations}");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            drop(parsed);
            checked.solved.validate().unwrap();
            let denied = source.replace("proc consume() [time, env]", "proc consume() [time]");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &denied);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &denied);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains("env")), "{:?}", checked.diagnostics);
            assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.message.contains("unknown producer")), "{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn generalized_map_comprehension_keeps_key_eligibility_and_value_relationships() {
        use crate::frontend::query::SolvedQuery;
        use crate::syntax::arena::{ArenaCompQualifier, ArenaExprKind};
        let declarations = "proc copied(values) { {key: value for {key, value} in values} }\nproc forwarded(values) { copied(values) }\n";
        for calls in ["let numbers: Map[Int] = forwarded({[\"name\"]: 7})\nlet words: Map[Int, Str] = forwarded({[7]: \"seven\"})\n", "let words: Map[Int, Str] = forwarded({[7]: \"seven\"})\nlet numbers: Map[Int] = forwarded({[\"name\"]: 7})\n"] {
            let source = format!("{declarations}{calls}");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert_eq!(checked.solved.comprehension_operations.len(), 1);
            let (identity, clause) = checked.solved.comprehension_operations.iter().next().unwrap();
            let identity = *identity;
            assert_eq!(identity.qualifier, 0);
            let ArenaExprKind::MapComp { qualifiers, .. } = parsed.arena.arena.expr(identity.expression.expression).kind else { panic!("the clause belongs to the original map comprehension") };
            let ArenaCompQualifier::For { iter, .. } = parsed.arena.arena.comp_qualifiers(qualifiers)[0] else { panic!("the actual generator owns its receiver") };
            let iterable = super::super::ExpressionIdentity { expression: iter, ..identity.expression };
            assert_eq!(checked.solved.graph.resolved(clause.operation.actual_arguments[0]).unwrap(), checked.solved.graph.resolved(checked.solved.expressions[&iterable]).unwrap());
            assert!(!checked.solved.operations.contains_key(&iterable));
            let copied = *checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == "copied").unwrap();
            assert_eq!(clause.operation.caller, Some(copied));
            let forwarded = *checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == "forwarded").unwrap();
            let forwarded_calls = checked.solved.calls.values().filter(|call| call.declaration == Some(forwarded)).collect::<Vec<_>>();
            assert_eq!(forwarded_calls.len(), 2);
            for call in forwarded_calls {
                let selections = call.requirements.iter().filter_map(|requirement| checked.solved.graph.candidate_evidence(*requirement).unwrap()).collect::<Vec<_>>();
                assert_eq!(selections.len(), 1);
                assert!(matches!(checked.solved.operation_catalog.candidate(&checked.solved.graph, selections[0].candidate).unwrap(), super::super::SolvedOperationAuthority::Language(metadata) if metadata.authority == "language.iteration.Map"));
            }
            drop(parsed);
            checked.solved.validate().unwrap();
            let before = checked.solved.graph.counters().clone();
            let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
            let answer = query.comprehension_operation(identity).unwrap();
            assert_eq!(answer.actual_arguments.len(), 1);
            assert_eq!(answer.binding.supplied_slots, vec![0]);
            query.comprehension_producer_flow(identity).unwrap();
            assert_eq!(checked.solved.graph.counters(), &before);
        }
    }

    #[test]
    fn generalized_delegation_retains_independent_pull_and_cancellation_permissions() {
        use super::{ProducerPath, ProducerEffects};
        use crate::sema::inference::{EffectSet, EffectSummary};
        let source = "stream delayed() [time, env] -> Stream[Int] { defer { let _ = env.get(\"SETTING\") }; let _ = time.now(); yield 1 }\nstream copied(values) { yield @values }\nlet rows = copied(delayed())\nproc accepted() [time, env] -> Unit { for item in rows { let _ = item; break } }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let rows = checked.solved.bindings.keys().find(|identity| matches!(parsed.arena.arena.binding_target(identity.target).kind, crate::syntax::arena::ArenaBindingTargetKind::Name(name) if name == "rows")).copied().unwrap();
        assert_eq!(checked.solved.binding_producers[&rows][&ProducerPath::default()], ProducerEffects { pull: EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0)), close: EffectSummary::Closed(EffectSet::ENV) });
        drop(parsed);
        checked.solved.validate().unwrap();
        let rejected = source.replace("proc accepted() [time, env]", "proc denied() [time]");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &rejected);
        let checked = Checker::check_arena(&parsed.arena, &rejected);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics);
    }

    #[test]
    fn list_delegation_keeps_reached_operand_creation_permissions() {
        use crate::frontend::query::SolvedQuery;
        use crate::sema::inference::{EffectSet, EffectSummary, TypeNode};
        use crate::syntax::arena::{ArenaBindingTargetKind, ArenaStmtKind};
        let source = "proc make_value() [time, env] -> List[Int] { let _ = time.now(); let _ = env.get(\"UNREAD\"); [7] }\nstream copied(values) [] -> Stream[Int] { yield @values }\nstream forwarded(values) [] -> Stream[Int] { yield @copied(values) }\nproc accepted() [time, env] -> Unit { let rows = forwarded(make_value()); for item in rows { let _ = item; break } }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(86), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let copied = *checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == "copied").unwrap();
        let body = parsed.arena.arena.function_def(copied.declaration).body;
        let statements = parsed.arena.arena.stmt_ids(parsed.arena.arena.block(body).statements).filter_map(|statement| match parsed.arena.arena.stmt(statement).kind { ArenaStmtKind::YieldDelegate(value) => Some((statement, value)), _ => None }).collect::<Vec<_>>();
        assert_eq!(statements.len(), 1);
        let (statement, value) = statements[0];
        let identity = super::StatementIdentity { source: SourceId::new(86), namespace: copied.namespace, statement };
        let operation = &checked.solved.statement_operations[&identity];
        assert_eq!(operation.caller, Some(copied));
        assert_eq!(checked.solved.statement_owners[&identity], copied);
        let value = super::super::ExpressionIdentity { source: identity.source, namespace: identity.namespace, expression: value };
        assert_eq!(checked.solved.graph.resolved(operation.actual_arguments[0]).unwrap(), checked.solved.graph.resolved(checked.solved.expressions[&value]).unwrap());
        let forwarded = *checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == "forwarded").unwrap();
        let calls = checked.solved.calls.values().filter(|call| call.declaration == Some(forwarded)).collect::<Vec<_>>();
        assert_eq!(calls.len(), 1);
        let TypeNode::Arrow(signature) = checked.solved.graph.node(calls[0].signature).unwrap() else { panic!("the stream call keeps its creation signature") };
        assert_eq!(checked.solved.graph.closed_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet::EMPTY));
        let selected = calls[0].requirements.iter().filter_map(|requirement| checked.solved.graph.candidate_evidence(*requirement).unwrap()).map(|evidence| {
            assert_eq!(checked.solved.graph.closed_effect_summary(evidence.effects).unwrap(), EffectSummary::Closed(EffectSet::EMPTY));
            let super::super::SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap() else { panic!("delegation uses canonical language authority") };
            metadata.authority
        }).collect::<std::collections::BTreeSet<_>>();
        assert_eq!(selected, ["language.yield_delegation.List"].into_iter().collect());
        let body = parsed.arena.arena.function_def(forwarded.declaration).body;
        let delegates = parsed.arena.arena.stmt_ids(parsed.arena.arena.block(body).statements).filter_map(|statement| match parsed.arena.arena.stmt(statement).kind { ArenaStmtKind::YieldDelegate(value) => Some((statement, value)), _ => None }).collect::<Vec<_>>();
        assert_eq!(delegates.len(), 1);
        let (statement, value) = delegates[0];
        let ground_identity = super::StatementIdentity { source: SourceId::new(86), namespace: forwarded.namespace, statement };
        let ground = &checked.solved.statement_operations[&ground_identity];
        assert_eq!(ground.caller, Some(forwarded));
        assert_eq!(checked.solved.statement_owners[&ground_identity], forwarded);
        let value = super::super::ExpressionIdentity { source: ground_identity.source, namespace: ground_identity.namespace, expression: value };
        assert_eq!(checked.solved.graph.resolved(ground.actual_arguments[0]).unwrap(), checked.solved.graph.resolved(checked.solved.expressions[&value]).unwrap());
        let evidence = checked.solved.graph.candidate_evidence(ground.requirement).unwrap().expect("the fixed Stream receiver has definition-owned ground evidence");
        assert_eq!(checked.solved.graph.closed_effect_summary(evidence.effects).unwrap(), EffectSummary::Closed(EffectSet::EMPTY));
        assert!(matches!(checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap(), super::super::SolvedOperationAuthority::Language(metadata) if metadata.authority == "language.yield_delegation.Stream"));
        let rows = *checked.solved.bindings.keys().find(|identity| matches!(parsed.arena.arena.binding_target(identity.target).kind, ArenaBindingTargetKind::Name(name) if name == "rows")).unwrap();
        assert_eq!(checked.solved.binding_producers[&rows][&super::ProducerPath::default()], super::ProducerEffects { pull: EffectSummary::Closed(EffectSet::EMPTY), close: EffectSummary::Closed(EffectSet::EMPTY) });
        drop(parsed);
        checked.solved.validate().unwrap();
        let before = checked.solved.graph.counters().clone();
        let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        assert_eq!(query.statement_operation(identity).unwrap().binding.supplied_slots, vec![0]);
        query.statement_producer_flow(identity).unwrap();
        assert_eq!(query.statement_operation(ground_identity).unwrap().binding.supplied_slots, vec![0]);
        assert_eq!(checked.solved.graph.counters(), &before);
        for (allowed, effect) in [("env", "time"), ("time", "env")] {
            let denied = source.replace("proc accepted() [time, env]", &format!("proc denied() [{allowed}]"));
            let parsed = Parser::parse_source_arena_only(SourceId::new(86), &denied);
            let checked = Checker::check_arena(&parsed.arena, &denied);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains(effect)), "{effect}: {:?}", checked.diagnostics);
            assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.message.contains("unknown producer")), "{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn generic_iteration_uses_a_fixed_result_boundary_for_outer_materialized_sources() {
        let calls = [
            "let source: Result[Map[Int], SourceFailure] = Ok({[\"name\"]: 7})\nlet mapped: Result[Int, SourceFailure] = forwarded(source)\n",
            "let text: Result[Str, SourceFailure] = Ok(\"word\")\nlet textual: Result[Int, SourceFailure] = forwarded(text)\n",
            "let octets: Result[Bytes, SourceFailure] = Ok(b\"word\")\nlet binary: Result[Int, SourceFailure] = forwarded(octets)\n",
        ];
        for body in ["for item in values { let _ = item }; 1", "let _ = [item for item in values]; 1", "let _ = {[\"item\"]: item for item in values}; 1"] {
            let declarations = format!("error SourceFailure = Missing(message: Str)\nproc visit(values) [error] -> Result[Int, SourceFailure] {{ {body} }}\nproc forwarded(values) [error] -> Result[Int, SourceFailure] {{ visit(values) }}\n");
            for reverse in [false, true] {
                let mut calls = calls.to_vec();
                if reverse { calls.reverse(); }
                let source = format!("{declarations}{}", calls.concat());
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                assert_eq!(checked.solved.statement_operations.len() + checked.solved.comprehension_operations.len(), 1);
                for declaration in checked.solved.declarations.values() {
                    let signature = checked.solved.graph.scheme(declaration.scheme).unwrap().body;
                    let crate::sema::inference::TypeNode::Arrow(signature) = checked.solved.graph.node(signature).unwrap() else { panic!("a checked declaration retains its arrow") };
                    let crate::sema::inference::TypeNode::Result(_, error) = checked.solved.graph.node(signature.result).unwrap() else { panic!("the written completion boundary remains a Result") };
                    assert!(matches!(checked.solved.graph.node(*error).unwrap(), crate::sema::inference::TypeNode::Atom(crate::sema::inference::Atom::ErrorFamily(name)) if *name == "SourceFailure"));
                }
                drop(parsed);
                checked.solved.validate().unwrap();
            }
            let wrong = format!("{declarations}error OtherFailure = Missing(message: Str)\nlet source: Result[Bytes, OtherFailure] = Ok(b\"word\")\nlet value = forwarded(source)\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &wrong);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &wrong);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch") || diagnostic.code.as_deref() == Some("check.try-error")), "{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn generic_iteration_keeps_written_data_returns_out_of_band() {
        for body in ["for item in values { let _ = item }; 1", "let _ = [item for item in values]; 1", "let _ = {[\"item\"]: item for item in values}; 1"] {
            let source = format!("error SourceFailure = Missing(message: Str)\nproc visit(values) [error] -> Int {{ {body} }}\nproc forwarded(values) [error] -> Int {{ visit(values) }}\nlet mapped: Result[Map[Int], SourceFailure] = Ok({{[\"name\"]: 7}})\nlet textual: Result[Str, SourceFailure] = Ok(\"word\")\nlet binary: Result[Bytes, SourceFailure] = Ok(b\"word\")\nlet map_value: Int = forwarded(mapped)\nlet text_value: Int = forwarded(textual)\nlet byte_value: Int = forwarded(binary)\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert!(checked.solved.declarations.values().all(|declaration| declaration.return_elaboration == ReturnElaboration::Value));
            drop(parsed);
            checked.solved.validate().unwrap();
        }
    }

    #[test]
    fn generic_iteration_local_capture_uses_its_own_error_port() {
        let calls = [
            "let mapped: Result[Map[Int], LocalFailure] = Ok({[\"name\"]: 7})\nlet map_value: Result[Int, LocalFailure] = forwarded(mapped)\n",
            "let textual: Result[Str, LocalFailure] = Ok(\"word\")\nlet text_value: Result[Int, LocalFailure] = forwarded(textual)\n",
            "let binary: Result[Bytes, LocalFailure] = Ok(b\"word\")\nlet byte_value: Result[Int, LocalFailure] = forwarded(binary)\n",
        ];
        for capture in ["try", "retry []"] {
            for body in ["for item in values { let _ = item }; 7", "let _ = [item for item in values]; 7", "let _ = {[\"item\"]: item for item in values}; 7"] {
                for reverse in [false, true] {
                    let mut calls = calls.to_vec();
                    if reverse { calls.reverse(); }
                    let source = format!("error LocalFailure = Missing(message: Str)\npure captured(values) {{ {capture} {{ {body} }} }}\npure forwarded(values) {{ captured(values) }}\n{}", calls.concat());
                    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                    let checked = Checker::check_arena(&parsed.arena, &source);
                    assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
                    let captured = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "captured").unwrap().1;
                    let crate::sema::inference::TypeNode::Arrow(signature) = checked.solved.graph.node(checked.solved.graph.scheme(captured.scheme).unwrap().body).unwrap() else { panic!("a local capture keeps its declaration signature") };
                    let crate::sema::inference::TypeNode::Result(_, error) = checked.solved.graph.node(signature.result).unwrap() else { panic!("local capture fixes its own Result shape") };
                    let operation = checked.solved.statement_operations.values().next().or_else(|| checked.solved.comprehension_operations.values().next().map(|clause| &clause.operation)).unwrap();
                    let crate::sema::inference::RequirementTemplate::Operation { call, .. } = checked.solved.graph.requirement_template(operation.requirement).unwrap() else { panic!("the source clause retains its canonical operation") };
                    let bound = checked.solved.graph.operation_call(call).unwrap().declared_error_bound.expect("local capture must own the iteration error port");
                    assert_eq!(checked.solved.graph.resolved(bound).unwrap(), checked.solved.graph.resolved(*error).unwrap());
                    assert!(checked.solved.declarations.values().all(|declaration| declaration.return_elaboration == ReturnElaboration::Value && checked.solved.graph.closed_effect_summary(declaration.effective_effects).unwrap() == crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY)));
                    drop(parsed);
                    checked.solved.validate().unwrap();
                }
            }
        }
    }

    #[test]
    fn generic_iteration_local_capture_joins_errors_and_preserves_permission_bounds() {
        for capture in ["try", "retry []"] {
            for body in ["for item in left { let _ = item }; for item in right { let _ = item }; 7", "assert true, \"checked\"; for item in right { let _ = item }; 7"] {
                let source = format!("error FirstFailure = Missing(message: Str)\nerror SecondFailure = Missing(message: Str)\npure captured(left, right) {{ {capture} {{ {body} }} }}\npure forwarded(left, right) {{ captured(left, right) }}\nlet left: Result[Map[Int], FirstFailure] = Ok({{[\"name\"]: 7}})\nlet right: Result[Bytes, SecondFailure] = Ok(b\"word\")\nlet output: Result[Int, Error] = forwarded(left, right)\n");
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
                for declaration in checked.solved.declarations.values() {
                    assert_eq!(checked.solved.graph.closed_effect_summary(declaration.effective_effects).unwrap(), crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY));
                }
                let captured = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "captured").unwrap().1;
                let principal = checked.solved.graph.scheme(captured.scheme).unwrap();
                let crate::sema::inference::TypeNode::Arrow(signature) = checked.solved.graph.node(principal.body).unwrap() else { panic!("a local capture keeps its declaration signature") };
                let crate::sema::inference::TypeNode::Result(_, error) = checked.solved.graph.node(signature.result).unwrap() else { panic!("the local error join retains its Result") };
                assert!(matches!(checked.solved.graph.node(*error).unwrap(), crate::sema::inference::TypeNode::Rigid { scope, .. } if *scope == captured.scheme), "the definition retains the conditional join instead of grounding it from these callers");
                let joins = principal.requirements.iter().filter_map(|requirement| match requirement {
                    crate::sema::inference::RequirementTemplate::ErrorJoin { join } => Some(checked.solved.graph.error_join(*join).unwrap()),
                    _ => None,
                }).collect::<Vec<_>>();
                assert_eq!(joins.len(), 1);
                assert_eq!(joins[0].inputs.len(), 2);
                assert_eq!(checked.solved.graph.resolved(joins[0].result).unwrap(), checked.solved.graph.resolved(*error).unwrap());
                assert!(joins[0].bound.is_none());
                let inputs = joins[0].inputs.iter().map(|input| checked.solved.graph.resolved(*input).unwrap()).collect::<Vec<_>>();
                assert_ne!(inputs[0], inputs[1]);
                for operation in checked.solved.statement_operations.values() {
                    let crate::sema::inference::RequirementTemplate::Operation { call, .. } = checked.solved.graph.requirement_template(operation.requirement).unwrap() else { panic!("iteration requirement") };
                    let bound = checked.solved.graph.operation_call(call).unwrap().declared_error_bound.unwrap();
                    assert!(inputs.contains(&checked.solved.graph.resolved(bound).unwrap()), "every original iteration error port remains an independent join input");
                }
                let (call, result) = checked.solved.expressions.iter().find(|(identity, _)| source.get(parsed.arena.arena.expr(identity.expression).span.range()) == Some("forwarded(left, right)")).unwrap();
                let call = *call;
                let crate::sema::inference::TypeNode::Result(_, actual_error) = checked.solved.graph.node(checked.solved.graph.resolved(*result).unwrap()).unwrap() else { panic!("the actual captured result") };
                assert!(matches!(checked.solved.graph.node(checked.solved.graph.resolved(*actual_error).unwrap()).unwrap(), crate::sema::inference::TypeNode::Atom(crate::sema::inference::Atom::Error)));
                drop(parsed);
                checked.solved.validate().unwrap();
                let before = checked.solved.graph.counters().clone();
                let query = crate::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
                assert_eq!(query.expression(call).unwrap().to_string(), "Result[Int, Error]");
                assert_eq!(checked.solved.graph.counters(), &before);
            }
            let declarations = format!("stream delayed() [time] -> Stream[Int] {{ let _ = time.now(); yield 1 }}\nproc captured(values) [time] {{ {capture} {{ for item in values {{ let _ = item; break }}; 7 }} }}\nlet output: Result[Int] = captured(delayed())\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &declarations);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &declarations);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            drop(parsed);
            checked.solved.validate().unwrap();
            let denied = declarations.replace("proc captured(values) [time]", "proc captured(values) []");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &denied);
            let checked = Checker::check_arena(&parsed.arena, &denied);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains("time")), "{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn generic_iteration_local_capture_requires_context_for_an_unobserved_error_type() {
        for capture in ["try", "retry []"] {
            let ungrounded = format!("stream quiet() [] -> Stream[Int] {{ yield 1 }}\npure captured(values) {{ {capture} {{ for item in values {{ let _ = item; break }}; 7 }} }}\nlet output = captured(quiet())\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &ungrounded);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &ungrounded);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.unsolved-relationship") && diagnostic.message.contains("annotation")), "{:?}", checked.diagnostics);
            assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.for-iterator") || diagnostic.message.contains("no supported operation signature")), "{:?}", checked.diagnostics);
            let grounded = ungrounded.replace("let output =", "let output: Result[Int] =");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &grounded);
            let checked = Checker::check_arena(&parsed.arena, &grounded);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            drop(parsed);
            checked.solved.validate().unwrap();
            let concrete = format!("pure captured(values: List[Int]) {{ {capture} {{ for item in values {{ let _ = item }}; 7 }} }}\nlet output: Result[Int] = captured([1])\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &concrete);
            let checked = Checker::check_arena(&parsed.arena, &concrete);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            drop(parsed);
            checked.solved.validate().unwrap();
        }
    }

    #[test]
    fn unresolved_iteration_does_not_train_lexical_result_completion_from_callers() {
        for source in ["proc copied(values) { for item in values { let _ = item }; 1 }\nlet value = copied(Ok({[\"name\"]: 7}))\n", "proc copied(values) { [item for item in values] }\nlet value = copied(Ok(\"word\"))\n"] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", checked.diagnostics);
        }
        for source in ["proc copied(values: Result[Map[Int]]) { for item in values { let _ = item }; 1 }\nlet value: Result[Int] = copied(Ok({[\"name\"]: 7}))\n", "proc copied(values: Result[Str]) { [item for item in values] }\nlet value: Result[List[Str]] = copied(Ok(\"word\"))\n"] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert!(checked.solved.declarations.values().all(|declaration| declaration.return_elaboration == ReturnElaboration::ImplicitResult));
            drop(parsed);
            checked.solved.validate().unwrap();
        }
    }

    #[test]
    fn generalized_iteration_rejects_a_noniterable_forwarded_receiver() {
        let source = "proc visit(values) { for item in values { let _ = item }; 1 }\nproc forwarded(values) { visit(values) }\nlet wrong: Int = forwarded(false)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", checked.diagnostics);
    }
}
