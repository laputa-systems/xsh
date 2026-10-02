use super::{ Checker, DeclarationIdentity, ExpressionIdentity, ReturnElaboration, SolvedCall, SolvedCallable, SolvedProjection, SolvedTypes, Type};
use crate::sema::inference::{Arrow, CallableKind, EffectSet, EffectSummary, Generalization, GeneralizationRoots, ComponentMember, InferenceError, Parameter, RequirementId, TypeId, TypeNode};
use crate::source::Span;
use crate::symbol::Name;
use crate::syntax::arena::{ArenaCallArg, ArenaExprKind, ArenaFunctionDef, ArenaProgram, BlockId, ExprId, FunctionDefId};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum ResolvedNominal {
    Tag(Name),
    ErrorFamily(Name),
    ErrorFacet(Name),
    ErrorVariant { family: Name, variant: Name },
}

impl ResolvedNominal {
    fn from_atom(atom: &crate::sema::inference::Atom) -> Option<Self> {
        match atom {
            crate::sema::inference::Atom::Tag(name) => Some(Self::Tag(*name)),
            crate::sema::inference::Atom::ErrorFamily(name) => Some(Self::ErrorFamily(*name)),
            crate::sema::inference::Atom::ProcessError => Some(Self::ErrorFamily(Name::PROCESS_ERROR)),
            crate::sema::inference::Atom::ErrorFacet(name) => Some(Self::ErrorFacet(*name)),
            crate::sema::inference::Atom::ErrorVariant { family, variant } => Some(Self::ErrorVariant { family: *family, variant: *variant }),
            _ => None,
        }
    }
}

pub(super) struct GenericDeclaration {
    pub signature: TypeId,
    pub params: Vec<TypeId>,
    pub result: TypeId,
    pub kind: CallableKind,
    pub fixed_return: bool,
    pub required_effects: EffectSummary,
    pub enclosing_owner: Option<DeclarationIdentity>,
    pub requirements: Vec<RequirementId>,
    pub completions: Vec<Type>,
    pub parameter_producer_flows: Vec<super::ProducerFlowId>,
    pub parameter_producers: Vec<super::ProducerProfile>,
    pub completion_producer_flows: Vec<super::ProducerFlowId>,
    pub propagated_error_producer_flows: Vec<super::ProducerFlowId>,
    pub return_producer_flow: Option<super::ProducerFlowId>,
    pub producer_effects: Option<super::ProducerEffects>,
    pub ambiguous_result_completion: bool,
    pub return_elaboration: Option<ReturnElaboration>,
}

#[derive(Default)]
pub(super) struct GenericState {
    pub facts: SolvedTypes<crate::sema::inference::InferenceContext>,
    pub registry: crate::sema::registry_graph::RegistryGraph,
    pub language_operations: crate::sema::operation_graph::OperationGraph,
    pub stage_graph: crate::sema::stage_graph::StageGraph,
    pub pending: BTreeMap<DeclarationIdentity, GenericDeclaration>,
    pub bodies: BTreeMap<BlockId, DeclarationIdentity>,
    pub names: BTreeMap<(Option<Name>, Name), DeclarationIdentity>,
    pub nominal_declarations: BTreeMap<(Option<Name>, ResolvedNominal), super::QualifiedNominalIdentity>,
    pub checking: BTreeSet<DeclarationIdentity>,
    pub generated: BTreeSet<DeclarationIdentity>,
    pub components: BTreeMap<DeclarationIdentity, std::sync::Arc<[DeclarationIdentity]>>,
    pub completed: BTreeSet<DeclarationIdentity>,
    pub rejected: BTreeSet<DeclarationIdentity>,
    pub diagnostics: Vec<super::Diagnostic>,
    pub producer_binding_versions: BTreeMap<super::BindingIdentity, u32>,
    pub producer_inputs: super::producer_eval::ProducerEvaluationInputs,
    pub run_argument_stack: Vec<Vec<super::run_operation::RunArgumentGuard>>,
}


pub(super) fn graph_effect_bits(effects: &[super::Effect]) -> EffectSet {
            let mut bits = EffectSet::EMPTY;
            for effect in effects {
                bits.0 |= match effect {
                    super::Effect::Fs => EffectSet::FS.0,
                    super::Effect::Net => EffectSet::NET.0,
                    super::Effect::Process => EffectSet::PROCESS.0,
                    super::Effect::Env => EffectSet::ENV.0,
                    super::Effect::Time => EffectSet::TIME.0,
                    super::Effect::Error => EffectSet::ERROR.0,
                    super::Effect::Io => EffectSet::IO.0,
                };
            }
            bits
        }

impl Checker {
    /// A recursive component shares monomorphic placeholders until all of its
    /// declaration bodies have contributed constraints. Traversal is iterative
    /// so deeply connected source graphs do not consume the host call stack.
    pub(super) fn prepare_graph_components(&mut self, arena: &ArenaProgram) {
        if !self.graph_generation { return; }
        let declarations: Vec<_> = self.generic.borrow().pending.keys().copied().collect();
        let dependencies = super::dependency::declaration_dependencies(arena, &declarations, &self.generic.borrow().names);
        let mut visited = BTreeSet::new();
        let mut order = Vec::new();
        for root in &declarations {
            let mut pending = vec![(*root, false)];
            while let Some((node, complete)) = pending.pop() {
                if complete { order.push(node); continue; }
                if !visited.insert(node) { continue; }
                pending.push((node, true));
                if let Some(children) = dependencies.get(&node) {
                    pending.extend(children.iter().rev().copied().map(|child| (child, false)));
                }
            }
        }
        let mut reverse: BTreeMap<_, BTreeSet<_>> = declarations.iter().map(|id| (*id, BTreeSet::new())).collect();
        for (node, children) in &dependencies {
            for child in children { reverse.entry(*child).or_default().insert(*node); }
        }
        visited.clear();
        let mut components = BTreeMap::new();
        for root in order.into_iter().rev() {
            if visited.contains(&root) { continue; }
            let mut pending = vec![root];
            let mut members = Vec::new();
            while let Some(node) = pending.pop() {
                if !visited.insert(node) { continue; }
                members.push(node);
                pending.extend(reverse.get(&node).into_iter().flatten().copied());
            }
            members.sort();
            let members: std::sync::Arc<[DeclarationIdentity]> = members.into();
            for member in members.iter() { components.insert(*member, members.clone()); }
        }
        self.generic.borrow_mut().components = components;
    }

    pub(super) fn normalize_graph_result_completion(
        &mut self, arena: &ArenaProgram, expression: Option<ExprId>, statement: Option<crate::syntax::arena::StmtId>,
        expected: Option<&Type>, actual: Type, span: Span,
    ) -> Type {
        let Some(owner) = self.current_generic else { return actual; };
        let def = arena.arena.function_def(owner.declaration);
        if def.return_ty_defaulted || !self.type_from_arena(arena, def.return_ty).is_result() { return actual; }
        let Some(expected @ Type::Result(payload, _)) = expected else { return actual; };
        if expected.is_result_unit() || actual.is_result() || matches!(actual, Type::Unknown | Type::Invalid) { return actual; }
        if let Type::Graph(id) = actual {
            let ambiguous = {
                let state = self.generic.borrow();
                state.facts.graph.resolved(id).and_then(|id| state.facts.graph.node(id)).is_ok_and(|node| matches!(node, TypeNode::Meta(_)))
            };
            if ambiguous {
                if self.graph_generation { self.generic.borrow_mut().pending.get_mut(&owner).unwrap().ambiguous_result_completion = true; }
                return actual;
            }
        }
        if self.graph_generation {
            self.expect_type(payload, &actual, span);
            match self.graph_type(expected, span) {
                Ok(result) => {
                    let mut state = self.generic.borrow_mut();
                    if let Some(expression) = expression {
                        let identity = ExpressionIdentity { source: arena.arena.expr(expression).span.source_id, namespace: self.current_namespace, expression };
                        state.facts.result_wrappings.insert(identity, result);
                    } else if let Some(statement) = statement {
                        let identity = super::StatementIdentity { source: arena.arena.stmt(statement).span.source_id, namespace: self.current_namespace, statement };
                        state.facts.result_statement_wrappings.insert(identity, result);
                    }
                }
                Err(error) => self.graph_error(span, error),
            }
        }
        expected.clone()
    }
    pub(super) fn record_graph_completion(&mut self, ty: &Type) {
        let Some(owner) = self.current_generic else { return; };
        if !self.graph_generation { return; }
        let ambiguous = if let Type::Graph(id) = ty {
            let state = self.generic.borrow();
            state.facts.graph.resolved(*id).and_then(|id| state.facts.graph.node(id)).is_ok_and(|node| matches!(node, TypeNode::Meta(_)))
        } else { false };
        let mut state = self.generic.borrow_mut();
        let declaration = state.pending.get_mut(&owner).unwrap();
        declaration.completions.push(ty.clone());
        declaration.ambiguous_result_completion |= ambiguous;
    }

    pub(super) fn record_argument_sources(&mut self, identity: ExpressionIdentity, arguments: &[crate::sema::arguments::ExpandedArgument]) -> Result<(), InferenceError> {
        if !self.graph_generation { return Ok(()); }
        let sources = arguments.iter().map(|argument| super::SolvedArgumentSource {
            entry_index: argument.entry_index, name: argument.name, value: argument.value, span: argument.span,
        }).collect();
        self.record_argument_source_rows(identity, sources)
    }

    pub(super) fn record_argument_source_rows(&mut self, identity: ExpressionIdentity, sources: Vec<super::SolvedArgumentSource>) -> Result<(), InferenceError> {
        if !self.graph_generation { return Ok(()); }
        let mut state = self.generic.borrow_mut();
        state.facts.graph.charge_source_fact_work(sources.len() as u64 + 1)?;
        if let Some(previous) = state.facts.argument_sources.get(&identity) {
            return if previous == &sources { Ok(()) } else { Err(InferenceError::Boundary("checked call argument sources changed after publication")) };
        }
        state.facts.graph.charge_source_fact_nodes(sources.len() as u64 + 1)?;
        state.facts.argument_sources.insert(identity, sources);
        Ok(())
    }
    pub(super) fn record_statement_position(&mut self, arena: &ArenaProgram, id: crate::syntax::arena::StmtId, position: super::StatementPosition) {
        let span = arena.arena.stmt(id).span;
        self.statement_positions.insert(span, position);
        if self.graph_generation {
            let identity = super::StatementIdentity { source: span.source_id, namespace: self.current_namespace, statement: id };
            let mut state = self.generic.borrow_mut();
            state.facts.statements.insert(identity, position);
            if let Some(owner) = self.current_generic { state.facts.statement_owners.insert(identity, owner); }
        }
    }
    fn close_unit_completion_positions(&mut self, arena: &ArenaProgram, body: BlockId) {
        let mut blocks = vec![body];
        while let Some(block) = blocks.pop() {
            let Some(tail) = arena.arena.stmt_ids(arena.arena.block(block).statements).last() else { continue; };
            self.record_statement_position(arena, tail, super::StatementPosition::Statement);
            match arena.arena.stmt(tail).kind {
                crate::syntax::arena::ArenaStmtKind::If { branches, else_block } => {
                    blocks.extend(arena.arena.if_branches(branches).iter().map(|branch| branch.block));
                    blocks.extend(else_block);
                }
                crate::syntax::arena::ArenaStmtKind::Match { arms, .. } => blocks.extend(arena.arena.match_arms(arms).iter().map(|arm| arm.block)),
                crate::syntax::arena::ArenaStmtKind::With { body, else_block, .. } => { blocks.push(body); blocks.push(else_block); }
                crate::syntax::arena::ArenaStmtKind::Expr(expression) => {
                    if let ArenaExprKind::ValueBlock(block) = arena.arena.expr(expression).kind { blocks.push(block); }
                }
                _ => {}
            }
        }
    }
    pub(super) fn finish_graph_declaration(&mut self, arena: &ArenaProgram, def: &ArenaFunctionDef, identity: DeclarationIdentity) {
        let span = arena.arena.span(arena.arena.block(def.body).span);
        let outcome = (|| {
            let (result, kind) = {
                let state = self.generic.borrow();
                let pending = &state.pending[&identity];
                (pending.result, pending.kind)
            };
            let mut elaboration = if kind == CallableKind::Stream { ReturnElaboration::UnitConsuming } else if def.return_ty_defaulted && !def.test_declaration && kind != CallableKind::Stream { ReturnElaboration::Value } else {
                let ty = self.type_from_arena(arena, def.return_ty);
                if ty == Type::Unit || ty.is_result_unit() { ReturnElaboration::UnitConsuming } else { ReturnElaboration::Value }
            };
            if kind != CallableKind::Stream && (!def.return_ty_defaulted || def.test_declaration) && self.type_from_arena(arena, def.return_ty).is_result()
                && elaboration != ReturnElaboration::UnitConsuming {
                let (completions, ambiguous) = {
                    let state = self.generic.borrow();
                    (state.pending[&identity].completions.clone(), state.pending[&identity].ambiguous_result_completion)
                };
                if ambiguous { return Err(InferenceError::Boundary("return payload-versus-Result interpretation needs an annotation")); }
                let _ = completions;
            }
            if def.return_ty_defaulted && !def.test_declaration && kind != CallableKind::Stream {
                let returns = self.inferred_returns.clone().unwrap_or_default();
                let mut payload = None;
                for (ty, contribution) in returns {
                    let ty = self.graph_type(&ty, contribution)?;
                    if let Some(previous) = payload {
                        let mut state = self.generic.borrow_mut();
                        let reason = state.facts.graph.reason(contribution, None)?;
                        state.facts.graph.unify(previous, ty, reason)?;
                    } else { payload = Some(ty); }
                }
                let payload = payload.ok_or(InferenceError::Unresolved(result))?;
                let value = self.graph_view(payload);
                // Completion roles follow the solved payload. A provisional
                // return variable must not leave Unit-producing tails classified
                // as values, while Bool and Result payloads remain ordinary values.
                if value == Type::Unit { self.close_unit_completion_positions(arena, def.body); }
                let propagated = !self.inferred_propagations.is_empty();
                let wrap = (propagated && !value.is_result()) || (kind == CallableKind::Proc && value == Type::Unit);
                let final_result = if wrap {
                    let error = self.inferred_propagations.first().map(|(ty, _)| ty.clone()).unwrap_or(Type::Error);
                    let error = if self.inferred_propagations.iter().any(|(other, _)| other != &error) { Type::Error } else { error };
                    let error = self.graph_type(&error, span)?;
                    elaboration = ReturnElaboration::ImplicitResult;
                    self.generic.borrow_mut().facts.graph.result(payload, error)?
                } else { payload };
                let mut state = self.generic.borrow_mut();
                let reason = state.facts.graph.reason(span, None)?;
                state.facts.graph.unify(result, final_result, reason)?;
            }
            self.generic.borrow_mut().pending.get_mut(&identity).unwrap().return_elaboration = Some(elaboration);
            self.finish_declaration_producer_flow(arena, def, identity);
            let mut state = self.generic.borrow_mut();
            state.generated.insert(identity);
            #[cfg(test)]
            Self::record_declaration_generation(identity);
            state.checking.remove(&identity);
            let component = state.components.get(&identity).cloned().unwrap_or_else(|| vec![identity].into());
            if component.iter().any(|member| !state.generated.contains(member)) {
                return Ok(result);
            }
            let component_members: BTreeSet<_> = component.iter().copied().collect();
            let mut incoming: BTreeMap<DeclarationIdentity, BTreeSet<DeclarationIdentity>> = BTreeMap::new();
            for call in state.facts.calls.values() {
                if let (Some(caller), Some(callee)) = (call.caller, call.declaration)
                    && component_members.contains(&caller) && component_members.contains(&callee) {
                    incoming.entry(callee).or_default().insert(caller);
                }
            }
            let mut propagated: BTreeMap<_, BTreeSet<_>> = component.iter().map(|member| (*member, state.pending[member].requirements.iter().copied().collect())).collect();
            let mut queue: std::collections::VecDeque<_> = component.iter().copied().collect();
            while let Some(callee) = queue.pop_front() {
                let requirements = propagated[&callee].clone();
                for caller in incoming.get(&callee).into_iter().flatten() {
                    let target = propagated.get_mut(caller).unwrap();
                    let previous = target.len();
                    target.extend(requirements.iter().copied());
                    if target.len() != previous { queue.push_back(*caller); }
                }
            }
            for (member, requirements) in propagated { state.pending.get_mut(&member).unwrap().requirements = requirements.into_iter().collect(); }
            let members: Vec<_> = component.iter().map(|member| {
                let pending = &state.pending[member];
                ComponentMember { root: pending.signature, requirements: pending.requirements.clone(), policy: Generalization::Allowed }
            }).collect();
            let roots: Vec<_> = component.iter().map(|member| GeneralizationRoots {
                captured_types: Vec::new(),
                effects: std::iter::once(state.pending[member].required_effects).chain(state.pending[member].producer_effects.into_iter().chain(state.pending[member].parameter_producers.iter().flat_map(|profile| profile.values().copied())).flat_map(|effects| [effects.pull, effects.close])).collect(),
            }).collect();
            let schemes = state.facts.graph.generalize_component_with_roots(&members, 0, None, &roots)?;
            #[cfg(test)]
            Self::record_declaration_generalization(&component);
            for (member, scheme) in component.iter().copied().zip(schemes) {
                let pending = &state.pending[&member];
                let signature = pending.signature;
                let kind = pending.kind;
                let requirements = pending.requirements.clone();
                let elaboration = pending.return_elaboration.ok_or(InferenceError::InvalidScheme)?;
                let binders = state.facts.graph.scheme_type_binders(scheme)?;
                let effect_binders = state.facts.graph.scheme_effect_binders(scheme)?;
                for call in state.facts.calls.values_mut() {
                    if call.declaration == Some(member) && call.caller.is_some_and(|caller| component.contains(&caller)) && call.substitutions.is_empty() {
                        call.substitutions = binders.clone();
                        call.effect_substitutions = effect_binders.clone();
                        call.requirements = requirements.clone();
                    }
                }
                let effects = match state.facts.graph.node(state.facts.graph.resolved(signature)?)? {
                    TypeNode::Arrow(arrow) => arrow.effects,
                    _ => return Err(InferenceError::InvalidScheme),
                };
                let body = arena.arena.function_def(member.declaration).body;
                let parameter_producers = state.pending[&member].parameter_producers.clone();
                let return_producers = state.pending[&member].producer_effects.map(|effects| BTreeMap::from([(super::ProducerPath::default(), effects)])).unwrap_or_default();
                let parameter_producer_flows = state.pending[&member].parameter_producer_flows.clone();
                let return_producer_flow = state.pending[&member].return_producer_flow;
                let required_effects = state.pending[&member].required_effects;
                let effective_effects = if kind == CallableKind::Stream {
                    arena.arena.function_def(member.declaration).effects.map(|effects| EffectSummary::Closed(graph_effect_bits(&arena.arena.effects(effects).collect::<Vec<_>>()))).unwrap_or(required_effects)
                } else { effects };
                state.facts.declarations.insert(member, SolvedCallable { scheme, signature, body, kind, return_elaboration: elaboration,
                    source_requirements: requirements.clone(),
                    parameter_producers, return_producers,
                    parameter_producer_flows, return_producer_flow,
                    effective_effects, required_effects });
                state.completed.insert(member);
            }
            Ok::<_, InferenceError>(result)
        })();
        match outcome {
            Ok(result) => {
                let ty = self.graph_view(result);
                self.function_return_types.insert(span, ty.clone());
                let kind = self.generic.borrow().pending[&identity].kind;
                if def.return_ty_defaulted && !def.test_declaration && kind != CallableKind::Stream && ty.annotation_source().is_some() {
                    if kind == CallableKind::Pure {
                        self.annotation_facts.push(super::AnnotationFact { kind: super::AnnotationFactKind::InferredPureReturn { body: span }, ty });
                    } else if self.current_exported && ty.is_result_unit() {
                        self.annotation_facts.push(super::AnnotationFact { kind: super::AnnotationFactKind::ExportedProcReturn { body: span }, ty });
                    }
                }
            }
            Err(error) => {
                { let mut state = self.generic.borrow_mut(); state.checking.remove(&identity); state.rejected.insert(identity); }
                self.graph_error(span, error);
            }
        }
    }

    pub(super) fn graph_call(&mut self, arena: &ArenaProgram, source: &str, callee: ExprId, args: &[ArenaCallArg], span: Span) -> Option<Type> {
        if self.stage_ground_call_adapter { return None; }
        let target = self.graph_callable_target(arena, callee)?;
        let declaration = target.declaration?;
        let def = arena.arena.function_def(declaration.declaration);
        let namespace = declaration.namespace;
        let name = def.name;
        if self.generic.borrow().rejected.contains(&declaration) { return Some(Type::Invalid); }
        let legacy_signature = if namespace == self.current_namespace {
            self.procs.get(&name).or_else(|| self.pures.get(&name)).or_else(|| self.streams.get(&name)).cloned()
        } else {
            let qualified = crate::symbol::QualifiedName::new(namespace?, name);
            self.qualified_procs.get(&qualified).or_else(|| self.qualified_pures.get(&qualified)).or_else(|| self.qualified_streams.get(&qualified)).cloned()
        };
        if let Some(signature) = &legacy_signature {
            if self.generic.borrow().pending[&declaration].kind != CallableKind::Pure {
                if self.in_pure { self.error(span, "effectful proc is not allowed in pure functions", "check.pure-effect"); }
                self.invalidate_mutable_narrowings();
            }
            self.record_callee_propagation(&signature.effects, &signature.return_ty, span);
        }
        let expression = self.current_expression?;
        let key = self.expression_identity(arena, expression);
        if !self.graph_generation || self.generic.borrow().facts.calls.contains_key(&key) {
            let result = self.generic.borrow().facts.calls.get(&key).and_then(|call| {
                let state = self.generic.borrow();
                match state.facts.graph.node(state.facts.graph.resolved(call.signature).ok()?).ok()? {
                    TypeNode::Arrow(arrow) => Some(arrow.result), _ => None,
                }
            });
            return Some(result.map(|ty| self.graph_view(ty)).unwrap_or(Type::Invalid));
        }
        if !self.generic.borrow().completed.contains(&declaration) && !self.generic.borrow().checking.contains(&declaration) {
            let def = arena.arena.function_def(declaration.declaration).clone();
            let pure = self.generic.borrow().pending[&declaration].kind == CallableKind::Pure;
            let saved_scopes = self.scopes.clone();
            if self.current_generic.is_some() { self.scopes.truncate(1); }
            let saved_namespace = self.current_namespace;
            self.current_namespace = declaration.namespace;
            if self.generic.borrow().pending[&declaration].kind == CallableKind::Stream { self.check_stream_function_arena(arena, source, &def); }
            else { self.check_function_arena(arena, source, &def, pure); }
            self.current_namespace = saved_namespace;
            self.scopes = saved_scopes;
        }
        let outcome = (|| {
            let (signature, requirements, requirement_origins, substitutions, effect_substitutions) = {
                let mut state = self.generic.borrow_mut();
                if let Some(completed) = state.facts.declarations.get(&declaration) {
                    let scheme = completed.scheme;
                    let reason = state.facts.graph.reason(span, None)?;
                    let level = if self.current_generic.is_some() { 1 } else { 0 };
                    let instantiation = state.facts.graph.instantiate(scheme, level, reason)?;
                    (instantiation.ty, instantiation.requirements, instantiation.requirement_origins, instantiation.substitutions, instantiation.effect_substitutions.into_iter().map(EffectSummary::Variable).collect())
                } else {
                    (state.pending[&declaration].signature, Vec::new(), Vec::new(), Vec::new(), Vec::new())
                }
            };
            let arrow = {
                let state = self.generic.borrow();
                let TypeNode::Arrow(arrow) = state.facts.graph.node(state.facts.graph.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme); };
                arrow.clone()
            };
            if self.in_pure && arrow.kind != CallableKind::Pure {
                self.error(span, "effectful proc is not allowed in pure functions", "check.pure-effect");
            }
            let result_producers = {
                let mut state = self.generic.borrow_mut();
                if let Some(completed) = state.facts.declarations.get(&declaration) {
                    let profile = completed.return_producers.clone();
                    let scheme = completed.scheme;
                    super::producer::instantiate_producer_profile(&mut state.facts.graph, &profile, scheme, &effect_substitutions)?
                } else { state.pending[&declaration].producer_effects.map(|effects| BTreeMap::from([(super::ProducerPath::default(), effects)])).unwrap_or_default() }
            };
            let schemas = legacy_signature.as_ref().map(|signature| signature.params.iter().map(|parameter| parameter.schema_expectation.clone()).collect::<Vec<_>>()).unwrap_or_default();
            let (actual_arguments, binding) = self.check_graph_callable_arguments(arena, source, signature, args, span, &schemas)?;
            let parameter_profiles = {
                let mut state = self.generic.borrow_mut();
                if let Some(completed) = state.facts.declarations.get(&declaration) {
                    let scheme = completed.scheme;
                    let profiles = completed.parameter_producers.clone();
                    profiles.iter().map(|profile| super::producer::instantiate_producer_profile(&mut state.facts.graph, profile, scheme, &effect_substitutions)).collect::<Result<Vec<_>, _>>()?
                } else { state.pending[&declaration].parameter_producers.clone() }
            };
            let argument_producers = self.bind_call_producer_arguments(arena, key, declaration, &binding, &parameter_profiles, &effect_substitutions, span)?;
            if arrow.kind != CallableKind::Pure { self.record_graph_effect_summary(arrow.effects, span); }
            let mut state = self.generic.borrow_mut();
            state.facts.graph.solve()?;
            let effect_pairs = if let Some(completed) = state.facts.declarations.get(&declaration) {
                state.facts.graph.scheme_effect_binders(completed.scheme)?.into_iter().zip(effect_substitutions.iter().copied()).collect()
            } else { Vec::new() };
            state.producer_inputs.call_effect_substitutions.insert(key, effect_pairs);
            state.producer_inputs.call_bindings.insert(key, binding.clone());
            state.producer_inputs.call_requirement_origins.insert(key, requirement_origins.clone());
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.extend(requirements.iter().copied()); }
            state.facts.calls.insert(key, SolvedCall {
                signature, declaration: Some(declaration), caller: self.current_generic,
                requirements, requirement_origins, substitutions, effect_substitutions, actual_arguments,
                argument_producers, result_producers,
                result_producer_flow: None,
                binding,
            });
            state.facts.expressions.insert(key, arrow.result);
            Ok::<_, InferenceError>(arrow.result)
        })();
        Some(match outcome { Ok(ty) => self.graph_view(ty), Err(error) => { self.graph_error(span, error); Type::Invalid } })
    }

    pub(super) fn graph_error(&mut self, span: Span, error: InferenceError) {
        let message = self.graph_error_message(&error);
        let registry_mismatch = if let InferenceError::UnsupportedOperation(requirement) = &error {
            matches!(self.generic.borrow().facts.graph.requirement_template(*requirement), Ok(crate::sema::inference::RequirementTemplate::Operation { .. }))
        } else { false };
        let direct_yield = if let InferenceError::UnsupportedOperation(requirement) = &error {
            matches!(self.generic.borrow().facts.graph.requirement_template(*requirement), Ok(crate::sema::inference::RequirementTemplate::Eligibility { predicate: crate::sema::inference::Eligibility::YieldItem, .. }))
        } else { false };
        let display_conversion = if let InferenceError::UnsupportedOperation(requirement) = &error {
            matches!(self.generic.borrow().facts.graph.requirement_template(*requirement), Ok(crate::sema::inference::RequirementTemplate::Eligibility { predicate: crate::sema::inference::Eligibility::Display, .. }))
        } else { false };
        let code = match &error {
            InferenceError::EffectViolation | InferenceError::OperationEffectViolation { .. } => "check.effect-violation",
            InferenceError::OperationErrorBoundViolation { .. } => "check.try-error",
            InferenceError::InvalidInvocation { problem: crate::sema::inference::InvocationProblem::CallableKind, .. } => "check.pure-effect",
            InferenceError::InvalidInvocation { problem: crate::sema::inference::InvocationProblem::TooManyArguments | crate::sema::inference::InvocationProblem::MissingArgument(_), .. } => "check.arity",
            InferenceError::InvalidInvocation { problem: crate::sema::inference::InvocationProblem::UnknownLabel(_) | crate::sema::inference::InvocationProblem::DuplicateArgument(_), .. } => "check.named-arg",
            InferenceError::InvalidInvocation { problem: crate::sema::inference::InvocationProblem::InvalidSplice, .. } => "check.call-splice",
            InferenceError::TypeMismatch { .. } => "check.type-mismatch",
            _ if direct_yield => "check.yield-stream",
            _ if display_conversion => "check.display-conversion",
            _ if registry_mismatch => "check.type-mismatch",
            _ => "check.type-relationship",
        };
        self.graph_boundary_error(span, &message, code);
    }

    pub(super) fn graph_boundary_error(&mut self, span: Span, message: &str, code: &str) {
        self.error(span, message, code);
        if self.graph_generation { self.generic.borrow_mut().diagnostics.push(self.diagnostics.last().unwrap().clone()); }
    }

    pub(super) fn graph_error_message(&self, error: &InferenceError) -> String {
        match error {
            InferenceError::OperationErrorBoundViolation { requirement, bound, projection } => {
                let state = self.generic.borrow();
                let graph = &state.facts.graph;
                let bound = graph.export_type(*bound).map(|ty| ty.to_string()).unwrap_or_else(|_| "the declared error type".to_string());
                let projected = match graph.requirement_template(*requirement) {
                    Ok(crate::sema::inference::RequirementTemplate::Operation { call, .. }) => graph.operation_call(call).ok().and_then(|call| match projection {
                        crate::sema::inference::OperationFailureProjection::ReceiverResultError => call.receiver,
                        crate::sema::inference::OperationFailureProjection::ArgumentResultError { argument } => call.arguments.get(*argument).copied().flatten(),
                    }).and_then(|actual| graph.resolved(actual).ok()).and_then(|actual| match graph.node(actual).ok()? { TypeNode::Result(_, error) => graph.export_type(*error).ok(), _ => None }),
                    _ => None,
                };
                projected.map_or_else(|| format!("operation error cannot propagate into declared error type {bound}"), |actual| format!("operation error {actual} cannot propagate into declared error type {bound}"))
            }
            InferenceError::OperationEffectViolation { required, available, .. } => {
                match (required, available) {
                    (Some(required), Some(available)) => format!("operation requires [{}], which exceed the available effects [{}]", super::infer_effects::graph_effects(EffectSummary::Closed(*required)).unwrap().iter().map(super::Effect::as_str).collect::<Vec<_>>().join(", "), super::infer_effects::graph_effects(EffectSummary::Closed(*available)).unwrap().iter().map(super::Effect::as_str).collect::<Vec<_>>().join(", ")),
                    _ => "operation requires latent or unrestricted effects beyond the available callable boundary".to_string(),
                }
            }
            InferenceError::InvalidInvocation { problem, .. } => {
                use crate::sema::inference::InvocationProblem as Problem;
                match problem {
                    Problem::NotCallable => "value does not have a checked callable signature".to_string(),
                    Problem::CallableKind => "effectful callable is not allowed in this callable context".to_string(),
                    Problem::UnknownLabel(name) => format!("unknown named parameter `{name}`"),
                    Problem::DuplicateArgument(name) => format!("parameter `{name}` supplied more than once"),
                    Problem::MissingArgument(name) => format!("missing required parameter `{name}`"),
                    Problem::TooManyArguments => "too many positional arguments".to_string(),
                    Problem::InvalidSplice => "positional splice must retain a checked List type".to_string(),
                    Problem::InvalidRest => "rest parameter must retain a checked List type".to_string(),
                }
            }
            InferenceError::TypeMismatch { left, right } => {
                let state = self.generic.borrow();
                let describe = |id| state.facts.graph.export_type(id).map(|ty| ty.to_string()).unwrap_or_else(|_| "a checked type relationship".to_string());
                format!("expected {}, found {}", describe(*left), describe(*right))
            }
            InferenceError::MissingField(field) => format!("record is missing required field `{field}`"),
            InferenceError::DuplicateLabel(field) | InferenceError::Lacks(field) => format!("record field `{field}` conflicts with its row relationship"),
            InferenceError::Occurs { .. } => "infinite type relationship: occurs check failed".to_string(),
            InferenceError::UnsupportedOperation(requirement) => {
                let state = self.generic.borrow();
                match state.facts.graph.requirement_template(*requirement) {
                    Ok(crate::sema::inference::RequirementTemplate::Add { left, right, .. }) => {
                        let describe = |ty| state.facts.graph.export_type(ty).map(|ty| ty.to_string()).unwrap_or_else(|_| "inferred operand".to_string());
                        format!("`+` does not support operand domains {} and {}", describe(left), describe(right))
                    }
                    Ok(crate::sema::inference::RequirementTemplate::Eligibility { predicate, ty }) => {
                        let domain = state.facts.graph.export_type(ty).map(|ty| ty.to_string()).unwrap_or_else(|_| "an inferred value".to_string());
                        let boundary = match predicate {
                            crate::sema::inference::Eligibility::MapKey => "a map key",
                            crate::sema::inference::Eligibility::YieldItem => "a direct yield value distinct from a producer",
                            crate::sema::inference::Eligibility::CommandTarget => "a command target",
                            crate::sema::inference::Eligibility::CommandArgv => "a checked command argument vector",
                            crate::sema::inference::Eligibility::Error => "an Error value",
                            crate::sema::inference::Eligibility::Display => "a printable scalar value",
                            crate::sema::inference::Eligibility::JsonCompatible => "JSON data",
                            crate::sema::inference::Eligibility::NonUnit => "a material value",
                            crate::sema::inference::Eligibility::Sortable => "a sortable value",
                            crate::sema::inference::Eligibility::SortableKey => "an ordering key",
                            crate::sema::inference::Eligibility::ArgvItem => "an argument vector item",
                            crate::sema::inference::Eligibility::ArgvExpansion => "an argument vector value or one list of items",
                            crate::sema::inference::Eligibility::CountKey => "a count key",
                            crate::sema::inference::Eligibility::Record => "a record value",
                        };
                        format!("{domain} cannot establish {boundary}")
                    }
                    Ok(crate::sema::inference::RequirementTemplate::EqualityCompatible { left, right }) => {
                        let describe = |ty| state.facts.graph.export_type(ty).map(|ty| ty.to_string()).unwrap_or_else(|_| "an inferred value".to_string());
                        format!("equality does not support operand domains {} and {}", describe(left), describe(right))
                    }
                    Ok(crate::sema::inference::RequirementTemplate::Operation { call, .. }) => {
                        let domains = state.facts.graph.operation_call(call).map(|call| call.receiver.iter().copied()
                            .chain(call.arguments.iter().flatten().copied()).map(|ty| state.facts.graph.export_type(ty)
                                .map(|ty| ty.to_string()).unwrap_or_else(|_| "an inferred value".to_string())).collect::<Vec<_>>());
                        match domains {
                            Ok(domains) => format!("no supported operation signature for {}", domains.join(", ")),
                            Err(_) => "operation needs a local type contract".to_string(),
                        }
                    }
                    Ok(crate::sema::inference::RequirementTemplate::ErrorJoin { join }) => {
                        let graph = &state.facts.graph;
                        match graph.error_join(join) {
                            Ok(join) => {
                                let describe = |ty| graph.export_type(ty).map(|ty| ty.to_string()).unwrap_or_else(|_| "an inferred error type".to_string());
                                let inputs = join.inputs.iter().copied().map(describe).collect::<Vec<_>>().join(", ");
                                let boundary = join.bound.map(|bound| format!(" within declared error boundary {}", describe(bound))).unwrap_or_default();
                                format!("reached error contributions {inputs} cannot establish joined error type {}{boundary}", describe(join.result))
                            }
                            Err(_) => "error join relationship is unavailable in this checked source bundle".to_string(),
                        }
                    }
                    Ok(crate::sema::inference::RequirementTemplate::CallableInvocation { .. }) => "arguments do not establish the callable invocation contract".to_string(),
                    Ok(crate::sema::inference::RequirementTemplate::EffectInclusion { .. }) => "callable effects exceed the captured effect boundary".to_string(),
                    Err(_) => "operation relationship is unavailable in this checked source bundle".to_string(),
                }
            }
            InferenceError::DisconnectedRequirement(_) => "operation needs a local annotation; its type is independent of the callable signature".to_string(),
            InferenceError::Unresolved(_) => "type relationship needs a local annotation".to_string(),
            InferenceError::Recovery(_) => "an earlier type error prevents establishing this relationship".to_string(),
            InferenceError::ScopeEscape => "generic type parameter escapes its declaring scope".to_string(),
            InferenceError::EffectViolation => "callable effects exceed the checked effect boundary".to_string(),
            InferenceError::Boundary(reason) | InferenceError::Limit(reason) => format!("type relationship cannot be established: {reason}"),
            InferenceError::InvalidScheme => "arguments do not establish a complete callable relationship".to_string(),
            InferenceError::KindMismatch => "type and record-row relationships cannot be interchanged".to_string(),
            InferenceError::ForeignHandle => "type relationship belongs to a different checked source bundle".to_string(),
        }
    }

    pub(super) fn freeze_solved_types(&mut self) -> std::sync::Arc<SolvedTypes> {
        self.retain_diagnostic_effect_facts();
        fn annotation_key(fact: &super::AnnotationFact) -> (u8, Span) {
            match fact.kind {
                super::AnnotationFactKind::Binding { span, .. } => (0, span),
                super::AnnotationFactKind::DefaultedParam { span, .. } => (1, span),
                super::AnnotationFactKind::InferredPureReturn { body } => (2, body),
                super::AnnotationFactKind::ExportedProcReturn { body } => (3, body),
            }
        }
        let mut annotations = std::mem::take(&mut self.annotation_facts);
        for fact in &mut annotations { fact.ty = self.resolved_graph_view(fact.ty.clone()); }
        annotations.sort_by_key(annotation_key);
        annotations.dedup_by(|left, right| annotation_key(left) == annotation_key(right));
        self.annotation_facts = annotations;
        self.reveal_types.sort_by_key(|diagnostic| diagnostic.labels.first().map(|label| label.span));
        self.reveal_types.dedup();
        let parameters = std::mem::take(&mut self.parameter_types);
        self.parameter_types = parameters.into_iter().map(|(span, ty)| (span, self.resolved_graph_view(ty))).collect();
        let returns = std::mem::take(&mut self.function_return_types);
        self.function_return_types = returns.into_iter().map(|(span, ty)| (span, self.resolved_graph_view(ty))).collect();
        let bindings = std::mem::take(&mut self.local_inference.checked_bindings);
        self.local_inference.checked_bindings = bindings.into_iter().map(|(span, ty)| (span, self.resolved_graph_view(ty))).collect();
        let expressions = std::mem::take(&mut self.expr_types);
        self.expr_types = expressions.into_iter().map(|(span, ty)| (span, self.resolved_graph_view(ty))).collect();
        let outcome = {
            let mut state = self.generic.borrow_mut();
            let mut facts = std::mem::take(&mut state.facts);
            facts.retain_operation_catalog(&state.registry, &state.language_operations, &state.stage_graph).and_then(|()| facts.freeze())
        };
        match outcome {
            Ok(facts) => std::sync::Arc::new(facts),
            Err(error) => {
                let message = self.graph_error_message(&error);
                self.error(Span::at(crate::source::SourceId::new(0), 0), &message, "check.unsolved-relationship");
                std::sync::Arc::new(SolvedTypes::default())
            }
        }
    }

    pub(super) fn resolved_graph_view(&self, ty: Type) -> Type {
        if !ty.contains_graph() { return ty; }
        match ty {
            Type::Graph(id) => self.graph_view(id),
            Type::List(item) => Type::List(Box::new(self.resolved_graph_view(*item))),
            Type::Stream(item) => Type::Stream(Box::new(self.resolved_graph_view(*item))),
            Type::Optional(item) => Type::Optional(Box::new(self.resolved_graph_view(*item))),
            Type::Result(ok, error) => Type::Result(Box::new(self.resolved_graph_view(*ok)), Box::new(self.resolved_graph_view(*error))),
            Type::Map(key, value) => Type::Map(Box::new(self.resolved_graph_view(*key)), Box::new(self.resolved_graph_view(*value))),
            Type::Record(fields) => Type::Record(fields.into_iter().map(|(name, ty)| (name, self.resolved_graph_view(ty))).collect()),
            ty => ty,
        }
    }

    pub(super) fn expression_identity(&self, arena: &ArenaProgram, expression: ExprId) -> ExpressionIdentity {
        ExpressionIdentity { source: arena.arena.expr(expression).span.source_id, namespace: self.current_namespace, expression }
    }

    pub(super) fn graph_type(&mut self, ty: &Type, span: Span) -> Result<TypeId, InferenceError> {
        let level = if self.current_generic.is_some() { 1 } else { 0 };
        let ty = self.type_constraints.resolve(ty).map_err(|_| InferenceError::Boundary("legacy ground view could not be resolved"))?;
        let imported = self.generic.borrow_mut().facts.graph.import_type(&ty, level, span)?;
        let mut state = self.generic.borrow_mut();
        let mut pending = vec![imported];
        let mut seen = BTreeSet::new();
        while let Some(ty) = pending.pop() {
            let ty = state.facts.graph.resolved(ty)?;
            if !seen.insert(ty) { continue; }
            match state.facts.graph.node(ty)?.clone() {
                TypeNode::Atom(atom) => if let Some(key) = ResolvedNominal::from_atom(&atom) {
                    let local = state.nominal_declarations.get(&(self.current_namespace, key)).copied();
                    let identity = local.or_else(|| {
                        let mut matching = state.nominal_declarations.iter().filter(|((_, candidate), _)| *candidate == key).map(|(_, identity)| *identity);
                        let first = matching.next()?;
                        matching.all(|identity| identity == first).then_some(first)
                    });
                    if let Some(identity) = identity {
                        if state.facts.nominals.get(&ty).is_some_and(|previous| *previous != identity) { return Err(InferenceError::ScopeEscape); }
                        state.facts.nominals.insert(ty, identity);
                    }
                },
                TypeNode::List(item) | TypeNode::Stream(item) | TypeNode::Optional(item) => pending.push(item),
                TypeNode::Result(left, right) | TypeNode::Map(left, right) => { pending.push(left); pending.push(right); }
                TypeNode::Record(row) => {
                    let row = state.facts.graph.row_data(row)?;
                    pending.extend(row.fields.iter().map(|field| field.ty));
                    pending.extend(row.tail);
                }
                TypeNode::Arrow(arrow) => { pending.extend(arrow.params.iter().map(|parameter| parameter.ty)); pending.push(arrow.result); }
                TypeNode::NativeCallable(callable) => pending.push(callable.signature),
                TypeNode::CallableChoice(signatures) => pending.extend(signatures),
                TypeNode::FiniteDomain(alternatives) => pending.extend(alternatives.iter().map(|alternative| alternative.ty)),
                TypeNode::Module(fields) => pending.extend(fields.iter().map(|field| field.ty)),
                _ => {}
            }
        }
        Ok(imported)
    }

    pub(super) fn graph_view(&self, ty: TypeId) -> Type {
        fn view(graph: &crate::sema::inference::InferenceContext, ty: TypeId, depth: usize) -> Type {
            if let Ok(ground) = graph.export_type(ty) { return ground; }
            if depth > 64 { return Type::Graph(ty); }
            let node = graph.resolved(ty).and_then(|resolved| graph.node(resolved));
            match node {
                Ok(TypeNode::List(item)) => Type::List(Box::new(view(graph, *item, depth + 1))),
                Ok(TypeNode::Stream(item)) => Type::Stream(Box::new(view(graph, *item, depth + 1))),
                Ok(TypeNode::Optional(item)) => Type::Optional(Box::new(view(graph, *item, depth + 1))),
                Ok(TypeNode::Result(ok, error)) => Type::Result(Box::new(view(graph, *ok, depth + 1)), Box::new(view(graph, *error, depth + 1))),
                Ok(TypeNode::Map(key, value)) => Type::Map(Box::new(view(graph, *key, depth + 1)), Box::new(view(graph, *value, depth + 1))),
                _ => Type::Graph(ty),
            }
        }
        view(&self.generic.borrow().facts.graph, ty, 0)
    }

    pub(super) fn graph_callable_target(&self, arena: &ArenaProgram, expression: ExprId) -> Option<super::SolvedExpressionCallable> {
        let retained = {
            let state = self.generic.borrow();
            let identity = self.expression_identity(arena, expression);
            state.facts.expression_callables.get(&identity).map(|callable| {
                let mut callable = callable.clone();
                callable.scheme = state.facts.expression_schemes.get(&identity).copied().or(callable.scheme);
                callable
            })
        };
        if self.generic.borrow().facts.registry_references.contains_key(&self.expression_identity(arena, expression)) { return retained; }
        let (namespace, name) = match arena.arena.expr(expression).kind {
            ArenaExprKind::Ident(name) => {
                if let Some(binding) = self.lookup(name) { return binding.graph_callable.clone().or(retained); }
                (self.current_namespace, name)
            }
            ArenaExprKind::Field { base, name } if name == "call" => return self.graph_callable_target(arena, base),
            ArenaExprKind::Field { base, name } => match arena.arena.expr(base).kind {
                ArenaExprKind::Ident(namespace) => {
                    let Some(namespace) = self.lookup(namespace).and_then(|binding| binding.static_namespace) else { return retained; };
                    (Some(namespace), name)
                }
                _ => return retained,
            },
            _ => return retained,
        };
        let state = self.generic.borrow();
        let Some(owner) = state.names.get(&(namespace, name)).copied() else { return retained; };
        let signature = state.pending.get(&owner)?.signature;
        Some(super::SolvedExpressionCallable { signature, scheme: state.facts.declarations.get(&owner).map(|declaration| declaration.scheme), declaration: Some(owner) })
    }

    pub(super) fn prepare_graph_callable_value(&mut self, arena: &ArenaProgram, source: &str, expression: ExprId) {
        if !self.graph_generation { return; }
        let Some(target) = self.graph_callable_target(arena, expression) else { return; };
        let Some(owner) = target.declaration else { return; };
        if self.generic.borrow().generated.contains(&owner) || self.generic.borrow().checking.contains(&owner) { return; }
        let def = arena.arena.function_def(owner.declaration).clone();
        let pure = self.generic.borrow().pending[&owner].kind == CallableKind::Pure;
        let saved_namespace = self.current_namespace;
        let saved_scopes = self.scopes.clone();
        if self.current_generic.is_some() && self.generic.borrow().pending[&owner].enclosing_owner.is_none() { self.scopes.truncate(1); }
        self.current_namespace = owner.namespace;
        let kind = self.generic.borrow().pending[&owner].kind;
        if kind == CallableKind::Stream { self.check_stream_function_arena(arena, source, &def); }
        else { self.check_function_arena(arena, source, &def, pure); }
        self.current_namespace = saved_namespace;
        self.scopes = saved_scopes;
    }

    pub(super) fn record_graph_binding(&mut self, target: crate::syntax::arena::BindingTargetId, ty: &Type, mutable: bool, span: Span) {
        if !self.graph_generation { return; }
        if matches!(ty, Type::Unknown | Type::Invalid) { return; }
        let outcome = self.graph_type(ty, span);
        match outcome {
            Ok(ty) => {
                let identity = super::BindingIdentity { source: span.source_id, namespace: self.current_namespace, target };
                self.generic.borrow_mut().facts.bindings.insert(identity, super::SolvedBinding { ty, scheme: None, owner: self.current_generic, mutable });
            }
            Err(error) => self.graph_error(span, error),
        }
    }

    pub(super) fn graph_declaration(&self, body: BlockId) -> Option<DeclarationIdentity> {
        self.generic.borrow().bodies.get(&body).copied()
    }

    pub(super) fn graph_method_receiver(&mut self, ty: Type, name: &str, span: Span) -> Type {
        let Type::Graph(receiver) = ty else { return ty; };
        if !self.graph_generation { return self.graph_view(receiver); }
        let receivers: Vec<_> = super::api_spec().method_entries().filter(|(_, methods)|
            methods.iter().any(|method| method.name == name)).map(|(receiver, _)| receiver).collect();
        let [receiver_kind] = receivers.as_slice() else { return Type::Graph(receiver); };
        let ground = match receiver_kind {
            super::MethodReceiver::Str => Type::Str,
            super::MethodReceiver::Bytes => Type::Bytes,
            super::MethodReceiver::Path => Type::Path,
            super::MethodReceiver::Int => Type::Int,
            super::MethodReceiver::Float => Type::Float,
            super::MethodReceiver::Status => Type::Status,
            super::MethodReceiver::Digest => Type::Digest,
            super::MethodReceiver::Regex => Type::Regex,
            super::MethodReceiver::EnvPathList => Type::EnvPathList,
            super::MethodReceiver::ProcessHandle => Type::ProcessHandle,
            super::MethodReceiver::NetJob => Type::NetJob,
            super::MethodReceiver::FsRoot => Type::FsRoot,
            _ => return Type::Graph(receiver),
        };
        self.graph_expect(&ground, &Type::Graph(receiver), span);
        self.graph_view(receiver)
    }

    pub(super) fn register_graph_declaration(&mut self, arena: &ArenaProgram, id: FunctionDefId, kind: CallableKind, fixed_return: bool) {
        let def = arena.arena.function_def(id);
        let span = arena.arena.span(arena.arena.block(def.body).span);
        let identity = DeclarationIdentity { source: span.source_id, namespace: self.current_namespace, declaration: id };
        if self.generic.borrow().pending.contains_key(&identity) { return; }
        if !self.graph_generation { return; }
        let result = (|| {
            let mut params = Vec::new();
            let mut arguments = Vec::new();
            for param in arena.arena.params(def.params) {
                let origin = arena.arena.span(param.span);
                let ty = if param.ty_defaulted {
                    let mut state = self.generic.borrow_mut();
                    let variable = state.facts.graph.fresh(1, origin)?;
                    if param.rest { state.facts.graph.list(variable)? }
                    else if param.default.is_some_and(|default| matches!(arena.arena.expr(default).kind, ArenaExprKind::Null)) { state.facts.graph.optional(variable)? }
                    else { variable }
                } else {
                    let ground = self.type_from_arena(arena, param.ty);
                    self.graph_type(&ground, origin)?
                };
                params.push(ty);
                arguments.push(Parameter { label: param.name, ty, defaulted: param.default.is_some(), rest: param.rest });
            }
            let return_ty = if def.return_ty_defaulted && !def.test_declaration && !fixed_return {
                let mut state = self.generic.borrow_mut();
                let result = state.facts.graph.fresh(1, span)?;
                if kind == CallableKind::Stream { state.facts.graph.stream(result)? } else { result }
            } else {
                let ground = self.type_from_arena(arena, def.return_ty);
                self.graph_type(&ground, span)?
            };
            let producer_effects = if kind == CallableKind::Stream {
                let upper = def.effects.map(|effects| graph_effect_bits(&arena.arena.effects(effects).collect::<Vec<_>>()));
                let mut state = self.generic.borrow_mut();
                Some(super::ProducerEffects {
                    pull: EffectSummary::Variable(state.facts.graph.fresh_derived_effect_at(1, upper)?),
                    close: EffectSummary::Variable(state.facts.graph.fresh_derived_effect_at(1, upper)?),
                })
            } else { None };
            let effects = if kind == CallableKind::Pure || kind == CallableKind::Stream { EffectSummary::Closed(EffectSet::EMPTY) }
                else if let Some(effects) = def.effects { EffectSummary::Closed(graph_effect_bits(&arena.arena.effects(effects).collect::<Vec<_>>())) }
                else if !def.test_declaration && def.name != "main" { EffectSummary::Variable(self.generic.borrow_mut().facts.graph.fresh_derived_effect_at(1, None)?) }
                else { EffectSummary::Unknown };
            let required_effects = if kind == CallableKind::Pure { effects }
                else if def.effects.is_none() && !def.test_declaration && def.name != "main" && kind != CallableKind::Stream { effects }
                else {
                    EffectSummary::Variable(self.generic.borrow_mut().facts.graph.fresh_derived_effect_at(1, None)?)
                };
            if let Some(producer) = producer_effects {
                let mut state = self.generic.borrow_mut();
                let reason = state.facts.graph.reason(span, None)?;
                state.facts.graph.include_effects(producer.pull, required_effects, reason)?;
                state.facts.graph.include_effects(producer.close, required_effects, reason)?;
            }
            let signature = self.generic.borrow_mut().facts.graph.arrow(Arrow {
                kind, params: arguments, result: return_ty,
                effects,
            })?;
            Ok::<_, InferenceError>(GenericDeclaration { signature, parameter_producers: vec![BTreeMap::new(); params.len()], params, result: return_ty, kind, fixed_return, required_effects, enclosing_owner: self.current_generic, requirements: Vec::new(), completions: Vec::new(), parameter_producer_flows: Vec::new(), completion_producer_flows: Vec::new(), propagated_error_producer_flows: Vec::new(), return_producer_flow: None, producer_effects, ambiguous_result_completion: false, return_elaboration: None })
        })();
        match result {
            Ok(declaration) => {
                if kind != CallableKind::Pure {
                    let inference_allowed = !def.test_declaration && def.name != "main";
                    self.diagnostic_effect_facts.insert(identity, super::FunctionEffectFact {
                        effective: def.effects.map(|effects| arena.arena.effects(effects).collect()),
                        required: None,
                        inferred: inference_allowed && def.effects.is_none(),
                        inference_allowed,
                        unknown_chain: vec![def.name.to_string()],
                    });
                }
                let mut state = self.generic.borrow_mut();
                let mut declaration = declaration;
                for index in 0..declaration.params.len() {
                    let facts = &mut state.facts;
                    let flow = facts.producer_flows.push(&mut facts.graph, super::ProducerFlowSource::Parameter { declaration: identity, index: index as u32 }, super::ProducerFlowKind::Parameter { declaration: identity, index: index as u32 });
                    match flow { Ok(flow) => declaration.parameter_producer_flows.push(flow), Err(error) => { drop(state); self.graph_error(span, error); return; } }
                }
                state.producer_inputs.declarations.insert(identity, super::producer_eval::ProducerEvaluationDeclaration {
                    parameters: declaration.parameter_producer_flows.clone(), result: None, defaults: BTreeMap::new(), enclosing: declaration.enclosing_owner,
                });
                state.pending.insert(identity, declaration);
                state.bodies.insert(def.body, identity);
                if !def.test_declaration { state.names.insert((self.current_namespace, def.name), identity); }
            }
            Err(error) => self.graph_error(span, error),
        }
    }

    pub(super) fn graph_expect(&mut self, expected: &Type, actual: &Type, span: Span) -> bool {
        if self.current_generic.is_none() && !expected.contains_graph() && !actual.contains_graph() { return false; }
        if !self.graph_generation { return true; }
        let result = (|| {
            let expected = self.graph_type(expected, span)?;
            let actual = self.graph_type(actual, span)?;
            let mut state = self.generic.borrow_mut();
            let reason = state.facts.graph.reason(span, None)?;
            state.facts.graph.assignable(expected, actual, reason)
        })();
        if let Err(error) = result { self.graph_error(span, error); }
        true
    }

    pub(super) fn record_graph_expression(&mut self, arena: &ArenaProgram, id: ExprId, ty: &Type) {
        if !self.graph_generation { return; }
        let identity = self.expression_identity(arena, id);
        if self.generic.borrow().facts.module_projections.contains_key(&identity)
            || self.generic.borrow().facts.registry_boundaries.contains_key(&identity)
            || self.generic.borrow().facts.registry_references.contains_key(&identity)
            || self.generic.borrow().facts.record_updates.contains_key(&identity)
            || self.generic.borrow().facts.constructor_applications.contains_key(&identity)
            || self.generic.borrow().facts.schema_validations.contains_key(&identity) { return; }
        if self.constructor_group_depth > 0 && ty.contains_inference() {
            if !self.pending_constructor_expressions.contains_key(&identity) {
                let charged = (|| {
                    let mut state = self.generic.borrow_mut();
                    state.facts.graph.charge_source_fact_nodes(1)?;
                    state.facts.graph.charge_source_fact_edges(1)?;
                    state.facts.graph.charge_source_fact_work(1)
                })();
                if let Err(error) = charged { self.graph_error(arena.arena.expr(id).span, error); return; }
            }
            self.pending_constructor_expressions.insert(identity, (ty.clone(), self.current_generic));
            return;
        }
        if let Some(callable) = self.graph_callable_target(arena, id) {
            let key = self.expression_identity(arena, id);
            let actual = match self.graph_type(ty, arena.arena.expr(id).span) {
                Ok(actual) => actual,
                Err(error) => { self.graph_error(arena.arena.expr(id).span, error); return; }
            };
            let mut state = self.generic.borrow_mut();
            state.facts.expressions.insert(key, actual);
            state.facts.expression_callables.insert(key, callable);
            if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(key, owner); }
            return;
        }
        if self.current_generic.is_none() && ty.contains_inference()
            && self.type_constraints.resolve(ty).is_ok_and(|ty| ty.contains_inference()) {
            return;
        }
        if self.current_generic.is_none() && self.graph_argument_depth == 0 && !ty.contains_graph() {
            if matches!(arena.arena.expr(id).kind, ArenaExprKind::Record(_)) {
                // Closed literal constructors keep their physical row shape even
                // when a later call receives them through a local binding.
                // Child types are projections of the already checked literal,
                // never the narrower row promised by a callable parameter.
                let mut pending = vec![(id, ty.clone())];
                while let Some((id, ty)) = pending.pop() {
                    let ArenaExprKind::Record(fields) = arena.arena.expr(id).kind else { continue; };
                    let Type::Record(record) = &ty else { continue; };
                    let result = self.graph_type(&ty, arena.arena.expr(id).span);
                    if let Ok(result) = result {
                        let mut state = self.generic.borrow_mut();
                        if state.facts.graph.export_type(result).is_ok() {
                            let identity = ExpressionIdentity { source: arena.arena.expr(id).span.source_id, namespace: self.current_namespace, expression: id };
                            state.facts.expressions.insert(identity, result);
                        }
                    }
                    for field in arena.arena.record_fields(fields) {
                        match field.kind {
                            crate::syntax::arena::ArenaRecordFieldKind::Named { name, value, .. } => {
                                if let Some(ty) = record.get(&name) { pending.push((value, ty.clone())); }
                            }
                            crate::syntax::arena::ArenaRecordFieldKind::Path { path, value, .. } => {
                                let mut field_ty = Some(&ty);
                                for name in arena.arena.names(path) {
                                    field_ty = field_ty.and_then(|ty| match ty { Type::Record(fields) => fields.get(&name), _ => None });
                                }
                                if let Some(ty) = field_ty { pending.push((value, ty.clone())); }
                            }
                            _ => {}
                        }
                    }
                }
            }
            return;
        }
        match self.graph_type(ty, arena.arena.expr(id).span) {
            Ok(ty) => {
                let key = self.expression_identity(arena, id);
                let mut state = self.generic.borrow_mut();
                state.facts.expressions.insert(key, ty);
                if matches!(state.facts.graph.resolved(ty).and_then(|ty| state.facts.graph.node(ty)), Ok(TypeNode::Arrow(_) | TypeNode::NativeCallable(_) | TypeNode::CallableChoice(_))) {
                    state.facts.expression_callables.insert(key, super::SolvedExpressionCallable { signature: ty, scheme: None, declaration: None });
                }
                if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(key, owner); }
            }
            Err(error) => self.graph_error(arena.arena.expr(id).span, error),
        }
    }

    pub(super) fn graph_projection(&mut self, arena: &ArenaProgram, id: ExprId, receiver: TypeId, field: Name) -> Type {
        let key = self.expression_identity(arena, id);
        if !self.graph_generation {
            return self.generic.borrow().facts.projections.get(&key).map(|fact| self.graph_view(fact.result)).unwrap_or(Type::Invalid);
        }
        let span = arena.arena.expr(id).span;
        let level = if self.current_generic.is_some() { 1 } else { 0 };
        let result = (|| {
            let mut state = self.generic.borrow_mut();
            let reason = state.facts.graph.reason(span, None)?;
            let result = state.facts.graph.require_field(receiver, field, level, reason)?;
            state.facts.projections.insert(key, SolvedProjection { receiver, field, result });
            Ok::<_, InferenceError>(result)
        })();
        match result { Ok(ty) => self.graph_view(ty), Err(error) => { self.graph_error(span, error); Type::Invalid } }
    }

    pub(super) fn graph_add(&mut self, arena: &ArenaProgram, id: ExprId, left: &Type, right: &Type) -> Type {
        let key = self.expression_identity(arena, id);
        let span = arena.arena.expr(id).span;
        if !self.graph_generation {
            return self.generic.borrow().facts.expressions.get(&key).map(|ty| self.graph_view(*ty)).unwrap_or(Type::Invalid);
        }
        let outcome = (|| {
            let left = self.graph_type(left, span)?;
            let right = self.graph_type(right, span)?;
            let mut state = self.generic.borrow_mut();
            let result = state.facts.graph.fresh(1, span)?;
            let reason = state.facts.graph.reason(span, None)?;
            let requirement = state.facts.graph.require_add(left, right, result, reason)?;
            state.facts.graph.solve()?;
            state.facts.additions.insert(key, requirement);
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.push(requirement); }
            Ok::<_, InferenceError>(result)
        })();
        match outcome { Ok(ty) => self.graph_view(ty), Err(error) => { self.graph_error(span, error); Type::Invalid } }
    }
}
