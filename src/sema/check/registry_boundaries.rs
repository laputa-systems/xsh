use super::{DeclarationIdentity, Type};
use crate::sema::inference::{CandidateId, EffectId, EffectSummary, RequirementId, SchemeId, TypeId};
use crate::sema::registry_graph::RegistrySchema;
use crate::modules::RuntimeOp;
use crate::symbol::Name;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RegistryValidationMode { Explicit, Contextual }

/// Literal argv children keep their checked types before the native mixed-item
/// join. Their guards authorize validation without converting path bytes.
#[derive(Clone, Debug)]
pub(crate) struct CommandArgvChild {
    pub source: super::ExpressionIdentity,
    pub actual: TypeId,
    pub splice: bool,
    pub requirement: RequirementId,
}

/// A source boundary refines a value only after its independent validation or
/// descriptor contract. Its canonical operation certificate remains unchanged.
#[derive(Clone, Debug)]
pub(crate) enum RegistryBoundaryKind {
    CliDescriptor { operation: RuntimeOp, plan: Arc<crate::modules::cli::CliDescriptorPlan> },
    HashAlgorithm { algorithm: Name },
    SchemaValidation { schema: RegistrySchema, mode: RegistryValidationMode },
    CommandArguments { argv_children: Vec<CommandArgvChild> },
}

#[derive(Clone, Debug)]
pub(crate) struct SolvedRegistryBoundary {
    pub requirement: Option<RequirementId>,
    pub input: TypeId,
    pub result: TypeId,
    pub caller: Option<DeclarationIdentity>,
    pub kind: RegistryBoundaryKind,
}

impl SolvedRegistryBoundary {
    pub(crate) fn retained_bytes(&self) -> usize {
        match &self.kind {
            RegistryBoundaryKind::CommandArguments { argv_children } => argv_children.capacity() * std::mem::size_of::<CommandArgvChild>(),
            _ => 0,
        }
    }
    pub(crate) fn shared_descriptor(&self) -> Option<&Arc<crate::modules::cli::CliDescriptorPlan>> {
        if let RegistryBoundaryKind::CliDescriptor { plan, .. } = &self.kind { Some(plan) } else { None }
    }
}

/// A native callable retains the canonical declaration and its fresh guards.
/// The signature alone cannot prove eligibility after an alias is generalized.
#[derive(Clone, Debug)]
pub(crate) struct SolvedRegistryReference {
    pub contract: RegistryReferenceContract,
    pub caller: Option<DeclarationIdentity>,
}

#[derive(Clone, Debug)]
pub(crate) enum RegistryReferenceContract {
    Arrow(RegistryArrowReference),
    Native { callable: TypeId, authority: crate::sema::inference::NativeAuthority },
}

#[derive(Clone, Debug)]
pub(crate) struct RegistryArrowReference {
    pub candidate: CandidateId,
    pub scheme: SchemeId,
    pub signature: TypeId,
    pub instantiated_requirements: Vec<RequirementId>,
    pub requirement_origins: Vec<(RequirementId, RequirementId)>,
    pub substitutions: Vec<TypeId>,
    pub effect_substitutions: Vec<EffectId>,
    pub effect_roots: Vec<EffectSummary>,
}

impl SolvedRegistryReference {
    pub(crate) fn value_type(&self) -> TypeId { match &self.contract { RegistryReferenceContract::Arrow(reference) => reference.signature, RegistryReferenceContract::Native { callable, .. } => *callable } }
    pub(crate) fn native_authority(&self) -> Option<crate::sema::inference::NativeAuthority> { match self.contract { RegistryReferenceContract::Native { authority, .. } => Some(authority), _ => None } }
    fn native_members(&self, graph: &crate::sema::inference::InferenceContext) -> Result<Vec<crate::sema::inference::NativeContractId>, crate::sema::inference::InferenceError> {
        use crate::sema::inference::NativeAuthority;
        Ok(match self.native_authority() { Some(NativeAuthority::Single(member)) => vec![member], Some(NativeAuthority::Family(family)) => graph.native_family_contract(family)?.members.clone(), None => Vec::new() })
    }
    pub(crate) fn requirements(&self, graph: &crate::sema::inference::InferenceContext) -> Result<Vec<RequirementId>, crate::sema::inference::InferenceError> {
        if let RegistryReferenceContract::Arrow(reference) = &self.contract { return Ok(reference.instantiated_requirements.clone()); }
        let mut requirements = Vec::new();
        for member in self.native_members(graph)? { requirements.extend_from_slice(&graph.native_contract(member)?.instance.requirements); }
        Ok(requirements)
    }
    pub(crate) fn candidates(&self, graph: &crate::sema::inference::InferenceContext) -> Result<Vec<CandidateId>, crate::sema::inference::InferenceError> {
        if let RegistryReferenceContract::Arrow(reference) = &self.contract { return Ok(vec![reference.candidate]); }
        self.native_members(graph)?.into_iter().map(|member| Ok(graph.native_contract(member)?.candidate)).collect()
    }
    pub(crate) fn certificates(&self, graph: &crate::sema::inference::InferenceContext) -> Result<Vec<crate::sema::inference::InstanceCertificate>, crate::sema::inference::InferenceError> {
        if let RegistryReferenceContract::Arrow(reference) = &self.contract { return Ok(vec![reference.certificate()]); }
        self.native_members(graph)?.into_iter().map(|member| Ok(graph.native_contract(member)?.certificate())).collect()
    }
    pub(crate) fn source_edges(&self) -> usize {
        match &self.contract { RegistryReferenceContract::Arrow(reference) => reference.source_edges(), RegistryReferenceContract::Native { .. } => 3 }
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        match &self.contract { RegistryReferenceContract::Arrow(reference) => reference.retained_bytes(), RegistryReferenceContract::Native { .. } => 0 }
    }
}

impl RegistryArrowReference {
    fn certificate(&self) -> crate::sema::inference::InstanceCertificate {
        crate::sema::inference::InstanceCertificate {
            scheme: self.scheme, signature: self.signature, substitutions: self.substitutions.clone(),
            effect_substitutions: self.effect_substitutions.clone(), effect_roots: self.effect_roots.clone(),
            requirement_origins: self.requirement_origins.clone(),
        }
    }
    fn source_edges(&self) -> usize {
        4 + self.instantiated_requirements.len() + self.requirement_origins.len() * 2
            + self.substitutions.len() + self.effect_substitutions.len() + self.effect_roots.len()
    }
    fn retained_bytes(&self) -> usize {
        self.instantiated_requirements.capacity() * std::mem::size_of::<RequirementId>()
            + self.requirement_origins.capacity() * std::mem::size_of::<(RequirementId, RequirementId)>()
            + self.substitutions.capacity() * std::mem::size_of::<TypeId>()
            + self.effect_substitutions.capacity() * std::mem::size_of::<EffectId>()
            + self.effect_roots.capacity() * std::mem::size_of::<EffectSummary>()
    }
}

impl super::Checker {
    pub(super) fn registry_json_guard_rejected(&mut self, error: &crate::sema::inference::InferenceError) -> Result<bool, crate::sema::inference::InferenceError> {
        use crate::sema::inference::{Eligibility, InferenceError, OperationBinding, RequirementTemplate, TypeNode};
        let InferenceError::UnsupportedOperation(requirement) = error else { return Ok(false); };
        let mut state = self.generic.borrow_mut();
        let graph = &mut state.facts.graph;
        let template = graph.requirement_template(*requirement)?;
        if matches!(template, RequirementTemplate::Eligibility { predicate: Eligibility::JsonCompatible, .. }) { return Ok(true); }
        let RequirementTemplate::Operation { family, call } = template else { return Ok(false); };
        let candidates = graph.family(family)?.to_vec();
        let call = graph.operation_call(call)?.clone();
        let mut guarded_actuals: Option<std::collections::BTreeSet<TypeId>> = None;
        for candidate in candidates {
            let template = graph.candidate(candidate)?.clone();
            let arguments = match call.binding {
                OperationBinding::Slots => call.arguments.clone(),
                OperationBinding::Invocation(invocation) => {
                    if template.has_receiver { return Ok(false); }
                    let invocation = graph.invocation_call(invocation)?.clone();
                    let signature = graph.scheme(template.scheme)?.body;
                    let TypeNode::Arrow(arrow) = graph.node(signature)? else { return Err(InferenceError::InvalidScheme); };
                    let count = arrow.params.len();
                    let kinds: Vec<_> = invocation.arguments.iter().map(|argument| argument.kind).collect();
                    let Ok(binding) = graph.plan_invocation_arguments(signature, &kinds) else { return Ok(false); };
                    if binding.dynamic.is_some() || binding.rest_slot.is_some() { return Ok(false); }
                    let mut arguments = vec![None; count];
                    for (argument, slot) in invocation.arguments.iter().zip(binding.supplied_slots) { arguments[slot] = Some(argument.ty); }
                    arguments
                }
            };
            let actual: std::collections::BTreeSet<_> = template.actual_eligibility.iter()
                .filter_map(|&(slot, predicate)| (predicate == Eligibility::JsonCompatible).then(|| arguments.get(slot).copied().flatten()).flatten()).collect();
            if let Some(common) = &mut guarded_actuals { *common = common.intersection(&actual).copied().collect(); }
            else { guarded_actuals = Some(actual); }
        }
        for actual in guarded_actuals.unwrap_or_default() {
            match graph.trial(|graph| graph.check_eligibility(Eligibility::JsonCompatible, actual)) {
                Err(InferenceError::Boundary(_)) => return Ok(true),
                Err(error) => return Err(error),
                Ok(_) => {},
            }
        }
        Ok(false)
    }

    pub(super) fn check_graph_registry_reference(
        &mut self, arena: &crate::syntax::arena::ArenaProgram, expression: crate::syntax::arena::ExprId,
        base: crate::syntax::arena::ExprId, name: Name, span: crate::source::Span,
    ) -> Option<Type> {
        use crate::syntax::arena::ArenaExprKind;
        use crate::sema::inference::InferenceError;
        if !self.graph_generation { return None; }
        let ArenaExprKind::Ident(module) = arena.arena.expr(base).kind else { return None; };
        if self.lookup(module).is_some() || !super::api_spec().module(&module.as_str())
            .is_some_and(|entry| entry.function_overloads(&name.as_str()).is_some()) { return None; }
        let identity = self.expression_identity(arena, expression);
        if let Some(reference) = self.generic.borrow().facts.registry_references.get(&identity) { return Some(Type::Graph(reference.value_type())); }
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let super::generic::GenericState { facts, registry, pending, producer_inputs, .. } = &mut *state;
            let family = registry.module_family(&mut facts.graph, &module.as_str(), &name.as_str(), span)?;
            let candidates = facts.graph.family(family)?.to_vec();
            let reason = facts.graph.reason(span, None)?;
            let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
            let contract = if candidates.len() != 1 {
                let mut instances = Vec::with_capacity(candidates.len());
                for candidate in candidates {
                    let metadata = registry.metadata(&facts.graph, candidate)?;
                    let template = facts.graph.candidate(candidate)?;
                    if !metadata.reference_family_member_supported(template) {
                        return Err(InferenceError::Boundary("native family references require compatible argument and producer preparation plans"));
                    }
                    let scheme = template.scheme;
                    if !facts.graph.scheme(scheme)?.quantifiers.is_empty() || !facts.graph.scheme(scheme)?.effect_quantifiers.is_empty() {
                        return Err(InferenceError::Boundary("native family references require complete canonical monotypes"));
                    }
                    instances.push((candidate, facts.graph.instantiate(scheme, level, reason)?));
                }
                let callable = facts.graph.native_family_callable(family, instances, reason)?;
                let crate::sema::inference::TypeNode::NativeCallable(wrapper) = facts.graph.node(callable)? else { return Err(InferenceError::InvalidScheme); };
                let [crate::sema::inference::CallableAuthority::Native { authority }] = wrapper.alternatives.as_slice() else { return Err(InferenceError::InvalidScheme); };
                RegistryReferenceContract::Native { callable, authority: *authority }
            } else {
                let candidate = candidates[0];
                let template = facts.graph.candidate(candidate)?.clone();
                let metadata = registry.metadata(&facts.graph, candidate)?;
                let protocol = metadata.reference_protocol(&template);
                if let crate::sema::registry_graph::RegistryReferenceProtocol::Boundary(_) = protocol {
                    return Err(InferenceError::Boundary("native callable references require an original argument or descriptor preparation plan"));
                }
                let scheme = template.scheme;
                let instance = facts.graph.instantiate(scheme, level, reason)?;
                if protocol == crate::sema::registry_graph::RegistryReferenceProtocol::Native {
                    let callable = facts.graph.native_callable(candidate, family, instance)?;
                    let crate::sema::inference::TypeNode::NativeCallable(wrapper) = facts.graph.node(callable)? else { return Err(InferenceError::InvalidScheme); };
                    let [crate::sema::inference::CallableAuthority::Native { authority }] = wrapper.alternatives.as_slice() else { return Err(InferenceError::InvalidScheme); };
                    RegistryReferenceContract::Native { callable, authority: *authority }
                } else {
                    RegistryReferenceContract::Arrow(RegistryArrowReference {
                        candidate, scheme, signature: instance.ty, instantiated_requirements: instance.requirements,
                        requirement_origins: instance.requirement_origins, substitutions: instance.substitutions,
                        effect_substitutions: instance.effect_substitutions, effect_roots: instance.effect_roots,
                    })
                }
            };
            let reference = SolvedRegistryReference { contract, caller: self.current_generic };
            facts.graph.charge_source_fact_nodes(1)?;
            let requirements = reference.requirements(&facts.graph)?;
            facts.graph.charge_source_fact_work(requirements.len() as u64 + reference.candidates(&facts.graph)?.len() as u64 + 1)?;
            if let Some(owner) = self.current_generic { pending.get_mut(&owner).ok_or(InferenceError::InvalidScheme)?.requirements.extend(requirements); }
            if let Some(authority) = reference.native_authority() {
                let flow = facts.producer_flows.push(&mut facts.graph, super::ProducerFlowSource::Expression(identity), super::ProducerFlowKind::NativeCallable { authority })?;
                producer_inputs.origins.insert(flow, span);
                facts.expression_producer_flows.insert(identity, flow);
            }
            let value_type = reference.value_type();
            facts.expressions.insert(identity, value_type);
            facts.expression_callables.insert(identity, super::SolvedExpressionCallable { signature: value_type, scheme: None, declaration: None });
            if let Some(owner) = self.current_generic { facts.expression_owners.insert(identity, owner); }
            facts.registry_references.insert(identity, reference);
            Ok(Type::Graph(value_type))
        })();
        Some(match outcome { Ok(ty) => ty, Err(error) => { self.graph_error(span, error); Type::Invalid } })
    }

    pub(super) fn graph_command_callable_parameters(&mut self, callable: TypeId) -> Result<Option<Vec<Name>>, crate::sema::inference::InferenceError> {
        use crate::sema::inference::{CallableAuthority, NativeAuthority, TypeNode};
        let mut state = self.generic.borrow_mut();
        let callable = state.facts.graph.resolved(callable)?;
        let TypeNode::NativeCallable(wrapper) = state.facts.graph.node(callable)? else { return Ok(None); };
        let signature = wrapper.signature;
        let authorities = wrapper.alternatives.clone();
        if authorities.is_empty() { return Ok(None); }
        let mut inspected = 0;
        for authority in authorities {
            let CallableAuthority::Native { authority } = authority else { return Ok(None); };
            let members = match authority {
                NativeAuthority::Single(member) => vec![member],
                NativeAuthority::Family(family) => state.facts.graph.native_family_contract(family)?.members.clone(),
            };
            for member in members {
                let candidate = state.facts.graph.native_contract(member)?.candidate;
                let metadata = state.registry.metadata(&state.facts.graph, candidate)?;
                if metadata.argument_check != crate::modules::signature::ApiArgCheck::CommandArgv
                    || metadata.operation != RuntimeOp::ProcessCommandArgv { return Ok(None); }
                inspected += 1;
            }
        }
        let TypeNode::Arrow(arrow) = state.facts.graph.node(signature)? else { return Err(crate::sema::inference::InferenceError::InvalidScheme); };
        let labels: Vec<_> = arrow.params.iter().map(|parameter| parameter.label).collect();
        state.facts.graph.charge_source_fact_work(inspected + labels.len() as u64)?;
        Ok(Some(labels))
    }

    pub(super) fn record_graph_command_arguments(
        &mut self, identity: super::ExpressionIdentity, requirement: RequirementId, children: Vec<CommandArgvChild>,
    ) -> Result<(), crate::sema::inference::InferenceError> {
        use crate::sema::inference::{InferenceError, RequirementTemplate};
        let mut state = self.generic.borrow_mut();
        let RequirementTemplate::Operation { call, .. } = state.facts.graph.requirement_template(requirement)? else { return Err(InferenceError::InvalidScheme); };
        let result = state.facts.graph.operation_call(call)?.result;
        state.facts.graph.charge_source_fact_nodes(1)?;
        state.facts.graph.charge_source_fact_work(children.len() as u64 * 4 + 3)?;
        state.facts.registry_boundaries.insert(identity, SolvedRegistryBoundary { requirement: Some(requirement), input: result, result,
            caller: self.current_generic, kind: RegistryBoundaryKind::CommandArguments { argv_children: children } });
        Ok(())
    }

    pub(super) fn check_graph_command_argument(
        &mut self, arena: &crate::syntax::arena::ArenaProgram, source: &str,
        value: crate::syntax::arena::ExprId, formal_slot: Option<usize>,
    ) -> (Type, Vec<CommandArgvChild>) {
        use crate::syntax::arena::ArenaExprKind;
        use crate::sema::inference::{Eligibility, InferenceError};
        let items = if formal_slot == Some(1) { match arena.arena.expr(value).kind { ArenaExprKind::List(items) => Some(items), _ => None } } else { None };
        let previous = self.expected_schema.take();
        let previous_literal = self.command_argv_literal_context;
        self.command_argv_literal_context = items.map(|_| value);
        let actual = self.check_expr_arena(arena, source, value, None);
        self.command_argv_literal_context = previous_literal;
        self.expected_schema = previous;
        let mut children = Vec::new();
        if let Some(items) = items {
            if items.is_empty() { self.graph_boundary_error(arena.arena.expr(value).span, "argv must include the child program name", "check.process-argv-empty"); }
            for item in arena.arena.list_elements(items) {
                let child_source = self.expression_identity(arena, item.value);
                let outcome: Result<CommandArgvChild, InferenceError> = (|| {
                    let mut state = self.generic.borrow_mut();
                    let actual = *state.facts.expressions.get(&child_source).ok_or(InferenceError::Boundary("native argv child has no original checked type"))?;
                    let reason = state.facts.graph.reason(arena.arena.expr(item.value).span, None)?;
                    let splice = item.splice_span.is_some();
                    let requirement = state.facts.graph.require_eligibility(if splice { Eligibility::CommandArgv } else { Eligibility::CommandTarget }, actual, reason)?;
                    state.facts.graph.solve()?;
                    if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).ok_or(InferenceError::InvalidScheme)?.requirements.push(requirement); }
                    Ok(CommandArgvChild { source: child_source, actual, splice, requirement })
                })();
                match outcome { Ok(child) => children.push(child), Err(InferenceError::UnsupportedOperation(_)) => {
                    self.graph_boundary_error(arena.arena.expr(item.value).span, "argv children must be Str or Path, with dynamic values validated by the native boundary", "check.type-mismatch");
                }, Err(error) => self.graph_error(arena.arena.expr(item.value).span, error) }
            }
        }
        if formal_slot == Some(13) { self.check_static_positive_call_int_arena(arena, value, "cpu_max must be positive"); }
        if formal_slot == Some(14) { self.check_static_accepted_exit_codes(arena, value); }
        (actual, children)
    }

    pub(super) fn check_graph_command_argv_call(
        &mut self, arena: &crate::syntax::arena::ArenaProgram, source: &str,
        args: &[crate::syntax::arena::ArenaCallArg], span: crate::source::Span, expected: Option<&Type>,
    ) -> Type {
        use crate::syntax::arena::ArenaCallArgKind as Argument;
        use crate::sema::inference::InferenceError;
        let Some(expression) = self.current_expression else {
            self.graph_error(span, InferenceError::Boundary("native command factory has no source identity"));
            return Type::Invalid;
        };
        let identity = self.expression_identity(arena, expression);
        let signatures = super::api_spec().module("process").unwrap().function_overloads("command_argv").unwrap();
        if signatures.iter().any(|signature| signature.arg_check != crate::modules::signature::ApiArgCheck::CommandArgv
            || signature.op != RuntimeOp::ProcessCommandArgv) {
            self.graph_error(span, InferenceError::InvalidScheme); return Type::Invalid;
        }
        let parameters = &signatures[0].params;
        let mut occupied = vec![false; parameters.len()];
        let mut next = 0;
        let mut checked = std::collections::BTreeMap::new();
        let mut children = Vec::new();
        for argument in args {
            let (value, slot) = match argument.kind {
                Argument::Named { name, value, .. } => (value, parameters.iter().position(|parameter| parameter.name == name.as_str().as_str())),
                Argument::Positional(value) => {
                    while next < occupied.len() && occupied[next] { next += 1; }
                    let slot = (next < occupied.len()).then_some(next); next += 1; (value, slot)
                }
                Argument::Splice { value, .. } | Argument::NamedSpread { value, .. } => {
                    self.graph_boundary_error(span, "command factories do not accept argument splices", "check.splice-target"); (value, None)
                }
            };
            if let Some(slot) = slot { occupied[slot] = true; }
            let (actual, argument_children) = self.check_graph_command_argument(arena, source, value, slot);
            checked.insert(value, actual);
            children.extend(argument_children);
        }
        let result = self.check_graph_module_call_with_checked_arguments(arena, source, "process", "command_argv", args, span, expected, checked);
        let operation = self.generic.borrow().facts.operations.get(&identity).cloned();
        if let Some(operation) = operation {
            if let Err(error) = self.record_graph_command_arguments(identity, operation.requirement, children) { self.graph_error(span, error); }
        }
        result
    }

    pub(super) fn record_graph_registry_validation(
        &mut self, arena: &crate::syntax::arena::ArenaProgram, expression: crate::syntax::arena::ExprId,
        value: crate::syntax::arena::ExprId, name: Name, mode: RegistryValidationMode, span: crate::source::Span,
    ) -> Option<Type> {
        if !self.graph_generation || xsh_registry::records::standard_record_type(&name.as_str()).is_none() { return None; }
        let identity = self.expression_identity(arena, expression);
        let input_identity = self.expression_identity(arena, value);
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let super::generic::GenericState { facts, registry, .. } = &mut *state;
            let schema = registry.schema(&mut facts.graph, &name.as_str(), span)?;
            let input = *facts.expressions.get(&input_identity).ok_or(crate::sema::inference::InferenceError::Boundary("schema validation input has no checked source type"))?;
            let error = facts.graph.atom(crate::sema::inference::Atom::Error)?;
            let result = facts.graph.result(schema.shape, error)?;
            facts.registry_boundaries.insert(identity, SolvedRegistryBoundary { requirement: None, input, result, caller: self.current_generic,
                kind: RegistryBoundaryKind::SchemaValidation { schema, mode } });
            facts.expressions.insert(identity, result);
            if let Some(owner) = self.current_generic { facts.expression_owners.insert(identity, owner); }
            facts.graph.export_type(result)
        })();
        Some(match outcome { Ok(ty) => ty, Err(error) => { self.graph_error(span, error); Type::Invalid } })
    }

    pub(super) fn check_graph_builtin_error_constructor(
        &mut self, arena: &crate::syntax::arena::ArenaProgram, source: &str,
        family: Name, variant: Name, args: &[crate::syntax::arena::ArenaCallArg], span: crate::source::Span,
    ) -> Option<Type> {
        use crate::syntax::arena::ArenaCallArgKind as Argument;
        if !self.graph_generation || !xsh_registry::errors::builtin_error_families().iter().any(|descriptor| descriptor.name == family.as_str().as_str()) { return None; }
        if !xsh_registry::errors::builtin_error_families().iter().any(|descriptor| descriptor.name == family.as_str().as_str()
            && descriptor.variants.iter().any(|descriptor| descriptor.name == variant.as_str().as_str())) {
            self.graph_boundary_error(span, "unknown error variant", "check.error-constructor"); return Some(Type::Invalid);
        }
        let outcome = (|| {
            let metadata = {
                let mut state = self.generic.borrow_mut();
                let super::generic::GenericState { facts, registry, .. } = &mut *state;
                registry.builtin_error_variant(&mut facts.graph, &family.as_str(), &variant.as_str(), span)?
            };
            let mut parameters: Vec<_> = metadata.parameters.iter().map(|parameter| crate::sema::types::CallableParamType {
                name: parameter.label, ty: self.graph_view(parameter.ty), defaulted: false, rest: false,
            }).collect();
            parameters.sort_by_key(|parameter| parameter.name);
            let mut checked = std::collections::BTreeMap::new();
            for argument in args {
                let value = match argument.kind {
                    Argument::Positional(value) | Argument::Named { value, .. } | Argument::NamedSpread { value, .. } => value,
                    Argument::Splice { value, .. } => {
                        self.graph_boundary_error(span, "error constructors do not accept argument splices", "check.splice-target"); value
                    }
                };
                checked.insert(value, self.check_expr_arena(arena, source, value, None));
            }
            let expanded = crate::sema::arguments::expand_named_arguments(arena, args, |value| checked.get(&value).cloned())
                .map_err(|_| crate::sema::inference::InferenceError::Boundary("error constructor arguments cannot be expanded"))?;
            let binding = match crate::sema::arguments::bind_static_arguments(&parameters, &expanded) {
                Ok(binding) => binding,
                Err(error) => { self.graph_boundary_error(error.span, &error.message, "check.error-constructor"); return Ok(Type::Invalid); }
            };
            for (argument, &slot) in expanded.iter().zip(&binding.argument_slots) { self.expect_type(&parameters[slot].ty, &argument.ty, argument.span); }
            if let Some(expression) = self.current_expression {
                let identity = self.expression_identity(arena, expression);
                let mut state = self.generic.borrow_mut();
                state.facts.expressions.insert(identity, metadata.result);
                if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(identity, owner); }
            }
            Ok(self.graph_view(metadata.result))
        })();
        Some(match outcome { Ok(ty) => ty, Err(error) => { self.graph_error(span, error); Type::Invalid } })
    }

    pub(super) fn check_graph_special_module_call(
        &mut self, arena: &crate::syntax::arena::ArenaProgram, source: &str,
        module: &str, name: &str, args: &[crate::syntax::arena::ArenaCallArg],
        span: crate::source::Span, expected: Option<&Type>,
    ) -> Type {
        use crate::syntax::arena::{ArenaCallArgKind as Argument, ArenaExprKind as Expression};
        use crate::sema::inference::{CallableKind, EffectSummary, InferenceError, OperationCall, TypeNode, Atom};
        use crate::modules::signature::{ApiArgCheck, SemanticRule};
        let Some(expression) = self.current_expression else {
            self.graph_error(span, InferenceError::Boundary("registry boundary requires its source expression"));
            return Type::Invalid;
        };
        let identity = self.expression_identity(arena, expression);
        if let Some(operation) = self.generic.borrow().facts.operations.get(&identity) {
            let result = self.generic.borrow().facts.registry_boundaries.get(&identity).map_or(operation.result, |boundary| boundary.result);
            return self.graph_view(result);
        }
        let outcome = (|| {
            let mut checked = std::collections::BTreeMap::new();
            for argument in args {
                let value = match argument.kind { Argument::Positional(value) | Argument::Named { value, .. }
                    | Argument::NamedSpread { value, .. } | Argument::Splice { value, .. } => value };
                let previous = self.expected_schema.take();
                let actual = self.check_expr_arena(arena, source, value, None);
                self.expected_schema = previous;
                checked.insert(value, actual);
            }
            let expanded = match crate::sema::arguments::expand_named_arguments(arena, args, |value| checked.get(&value).cloned()) {
                Ok(expanded) => expanded,
                Err(error) => { self.graph_boundary_error(error.span, &error.message, "check.named-spread"); return Ok(Type::Invalid); }
            };
            let (family, binding, parameters, hash_algorithm, contract) = {
                let mut state = self.generic.borrow_mut();
                let super::generic::GenericState { facts, registry, .. } = &mut *state;
                let family = registry.module_family(&mut facts.graph, module, name, span)?;
                let mut allowed = Vec::new();
                let mut selected_binding = None;
                let mut selected_parameters = None;
                let mut hash_algorithm = None;
                let mut binding_error = None;
                let mut forbidden_kind = false;
                let mut contract = None;
                for candidate in facts.graph.family(family)?.to_vec() {
                    let metadata = registry.metadata(&facts.graph, candidate)?;
                    if self.in_pure && metadata.kind != CallableKind::Pure { forbidden_kind = true; continue; }
                    let parameters: Vec<_> = metadata.parameters.iter().map(|parameter| crate::sema::types::CallableParamType {
                        name: parameter.label, ty: Type::Invalid, defaulted: parameter.defaulted, rest: false,
                    }).collect();
                    let mut arguments = expanded.clone();
                    if metadata.argument_check == ApiArgCheck::HashVerifyFile {
                        if arguments.len() != 2 {
                            drop(state); self.graph_boundary_error(span, "verify_file requires path and a named checksum", "check.arity"); return Ok(Type::Invalid);
                        }
                        let Some(algorithm) = arguments[1].name.filter(|name| matches!(name.as_str().as_str(), "md5" | "sha1" | "sha256" | "sha512")) else {
                            drop(state); self.graph_boundary_error(arguments[1].span, "checksum must use a supported named algorithm", "check.named-arg"); return Ok(Type::Invalid);
                        };
                        arguments[1].name = Some(parameters[1].name);
                        hash_algorithm = Some(algorithm);
                    }
                    match crate::sema::arguments::bind_static_arguments(&parameters, &arguments) {
                        Ok(binding) => {
                            if selected_binding.as_ref().is_some_and(|previous: &crate::sema::arguments::StaticArgumentBinding| previous != &binding) {
                                return Err(InferenceError::Boundary("registry candidates disagree on supplied source slots"));
                            }
                            let authority = (metadata.semantic_rule, metadata.operation);
                            if contract.is_some_and(|previous| previous != authority) { return Err(InferenceError::Boundary("registry candidates disagree on source boundary authority")); }
                            contract = Some(authority);
                            selected_binding = Some(binding); selected_parameters = Some(parameters); allowed.push(candidate);
                        }
                        Err(error) => binding_error = Some(error),
                    }
                }
                if allowed.is_empty() {
                    drop(state);
                    if let Some(error) = binding_error { self.graph_boundary_error(error.span, &error.message, "check.named-arg"); }
                    else if forbidden_kind { self.graph_boundary_error(span, "effectful module API is not allowed in pure functions", "check.pure-effect"); }
                    return Ok(Type::Invalid);
                }
                for name in self.wire_enums.mappings.keys() { facts.graph.allow_wire_tag(*name)?; }
                (facts.graph.register_family(&allowed)?, selected_binding.unwrap(), selected_parameters.unwrap(), hash_algorithm, contract.unwrap())
            };
            let descriptor = match contract.0 {
                SemanticRule::CliDescriptor => crate::modules::cli::descriptor_argument(args)
                    .and_then(|schema| self.prepared_constants.cli_descriptor_plan(&arena.arena, schema, contract.1 == RuntimeOp::CliApplet)),
                SemanticRule::CliCommands => crate::modules::cli::command_descriptor_sources(&expanded, &binding.argument_slots,
                    &parameters.iter().map(|parameter| parameter.name).collect::<Vec<_>>())
                    .and_then(|(commands, fallback)| self.prepared_constants.cli_commands_plan(&arena.arena, commands, fallback)),
                _ => None,
            };
            let descriptor = match descriptor {
                Some(Ok(plan)) => Some(plan),
                Some(Err(error)) => { self.graph_boundary_error(error.span.unwrap_or(span), &error.message, "check.cli-descriptor"); return Ok(Type::Invalid); }
                None => None,
            };
            let refined = descriptor.as_ref().map(|plan| self.graph_type(&plan.return_type(contract.1 == RuntimeOp::CliParseFull), span)).transpose()?;
            let mut actual_arguments = Vec::new();
            let mut supplied = vec![None; parameters.len()];
            let mut argument_coercions = Vec::new();
            for (index, (argument, &slot)) in expanded.iter().zip(&binding.argument_slots).enumerate() {
                let actual = self.graph_type(&argument.ty, argument.span)?;
                actual_arguments.push(actual);
                let path_slot = {
                    let state = self.generic.borrow();
                    state.facts.graph.family(family)?.iter().all(|candidate| {
                        state.facts.graph.candidate(*candidate).and_then(|candidate| state.facts.graph.scheme(candidate.scheme))
                            .and_then(|scheme| state.facts.graph.node(scheme.body)).is_ok_and(|node| matches!(node,
                                TypeNode::Arrow(arrow) if arrow.params.get(slot).is_some_and(|parameter|
                                    matches!(state.facts.graph.node(parameter.ty), Ok(TypeNode::Atom(Atom::Path))))))
                    })
                };
                supplied[slot] = if path_slot && matches!(argument.value, crate::sema::arguments::ArgumentValueSource::Expression(value)
                    if matches!(arena.arena.expr(value).kind, Expression::Str(_))) {
                    argument_coercions.push((index, super::RegistryArgumentCoercion::PathLikeToPath));
                    Some(self.graph_type(&Type::Path, argument.span)?)
                } else { Some(actual) };
            }
            let expected = expected.filter(|ty| !matches!(ty, Type::Unit | Type::Unknown | Type::Invalid)).map(|ty| self.graph_type(ty, span)).transpose()?;
            let mut state = self.generic.borrow_mut();
            let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
            let output_effect_bindings = { let super::generic::GenericState { facts, registry, .. } = &mut *state; registry.output_effect_bindings(&mut facts.graph, family, level)? };
            let graph = &mut state.facts.graph;
            let result = graph.fresh(level, span)?;
            let effects = EffectSummary::Variable(graph.fresh_execution_effect(None)?);
            let reason = graph.reason(span, None)?;
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: supplied, result, effects,
                effect_bindings: Vec::new(), output_effect_bindings }, reason)?;
            graph.solve()?;
            let expression_result = refined.unwrap_or(result);
            if let Some(expected) = expected {
                let contextual_result = match (graph.node(graph.resolved(expected)?)?, graph.node(graph.resolved(expression_result)?)?) {
                    (TypeNode::Result(_, _) | TypeNode::Meta(_) | TypeNode::Rigid { .. }, _) => expression_result,
                    (_, TypeNode::Result(value, _)) => *value,
                    _ => expression_result,
                };
                if let (Ok(actual), Ok(wanted)) = (graph.export_type(contextual_result), graph.export_type(expected))
                    && actual.any_flows_to_concrete(&wanted) {
                    drop(state); self.graph_boundary_error(span, "unchecked operation result requires explicit validation", "check.dynamic-boundary"); return Ok(Type::Invalid);
                }
                graph.assignable(expected, contextual_result, reason)?; graph.solve()?;
            }
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.push(requirement); }
            state.facts.operations.insert(identity, super::SolvedOperation { requirement, result, effects, receiver: None, actual_arguments,
                argument_coercions, binding: super::CallBinding { dynamic: None, supplied_slots: binding.argument_slots.clone(), default_slots: binding.omitted_slots.clone(), rest_slot: None }, caller: self.current_generic });
            state.facts.expressions.insert(identity, expression_result);
            if let Some(plan) = &descriptor {
                state.facts.registry_boundaries.insert(identity, SolvedRegistryBoundary { requirement: Some(requirement), input: result,
                    result: expression_result, caller: self.current_generic, kind: RegistryBoundaryKind::CliDescriptor { operation: contract.1, plan: plan.clone() } });
            } else if let Some(algorithm) = hash_algorithm {
                state.facts.registry_boundaries.insert(identity, SolvedRegistryBoundary { requirement: Some(requirement), input: result,
                    result, caller: self.current_generic, kind: RegistryBoundaryKind::HashAlgorithm { algorithm } });
            }
            if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(identity, owner); }
            drop(state);
            self.record_graph_effect_summary(effects, span);
            self.record_registry_operation_producer_flow(arena, expression, requirement, None, &expanded, &binding, span)?;
            if descriptor.is_some() && let Some(flow) = self.push_source_producer_flow(super::ProducerFlowSource::Expression(identity), super::ProducerFlowKind::Empty, span) {
                self.generic.borrow_mut().facts.expression_producer_flows.insert(identity, flow);
            }
            Ok(self.graph_view(expression_result))
        })();
        match outcome { Ok(ty) => ty, Err(error) => {
            match self.registry_json_guard_rejected(&error) {
                Ok(true) => self.graph_boundary_error(span, "value is not JSON-compatible; convert Path, Bytes, Status, Result, and errors explicitly", "check.json-compatible"),
                Ok(false) => self.graph_error(span, error),
                Err(error) => self.graph_error(span, error),
            }
            Type::Invalid
        } }
    }

    pub(super) fn check_graph_registry_schema(&mut self, name: Name, span: crate::source::Span) -> Option<Type> {
        if xsh_registry::records::standard_record_type(&name.as_str()).is_none() { return None; }
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let super::generic::GenericState { facts, registry, .. } = &mut *state;
            let schema = registry.schema(&mut facts.graph, &name.as_str(), span)?;
            facts.graph.export_type(schema.shape)
        })();
        Some(match outcome { Ok(ty) => ty, Err(error) => { self.graph_error(span, error); Type::Invalid } })
    }
}

#[cfg(test)]
mod tests {
    use crate::sema::check::{Checker, Type};
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    fn type_source(ty: &crate::sema::types::Type, declarations: &mut Vec<String>) -> String {
        use crate::sema::types::Type;
        match ty {
            Type::Record(fields) if !fields.is_empty() => {
                let fields = fields.iter().map(|(name, ty)| format!("{name}: {}", type_source(ty, declarations))).collect::<Vec<_>>().join(", ");
                let label = format!("SourceRecord{}", declarations.len());
                declarations.push(format!("type {label} = {{{fields}}}\n")); label
            },
            Type::List(item) => format!("List[{}]", type_source(item, declarations)),
            Type::Stream(item) => format!("Stream[{}]", type_source(item, declarations)),
            Type::Optional(item) => format!("{}?", type_source(item, declarations)),
            Type::Result(item, error) => format!("Result[{}, {}]", type_source(item, declarations), type_source(error, declarations)),
            Type::Map(key, item) => format!("Map[{}, {}]", type_source(key, declarations), type_source(item, declarations)),
            _ => ty.to_string(),
        }
    }

    fn contains_dynamic_module(ty: &crate::sema::types::Type) -> bool {
        use crate::sema::types::Type;
        match ty {
            Type::DynamicModule => true,
            Type::List(item) | Type::Stream(item) | Type::Optional(item) => contains_dynamic_module(item),
            Type::Result(item, error) | Type::Map(item, error) => contains_dynamic_module(item) || contains_dynamic_module(error),
            Type::Record(fields) => fields.values().any(contains_dynamic_module),
            _ => false,
        }
    }

    #[test]
    fn source_registry_named_spreads_keep_original_field_arguments() {
        let source = "pure appended(values, item) { values.push(...{item: item}) }\npure added(values: Map[Bool]) -> Map[Bool] { set.add(...{set: values, item: \"other\"}) }\nlet first: List[Int] = appended([1], 2)\nlet second: List[Str] = appended([\"one\"], \"two\")\nlet result = added({one: true})\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.operations.len(), 2);
        for identity in checked.solved.operations.keys() { let _ = parsed.arena.arena.expr(identity.expression); }
        for identity in checked.solved.expressions.keys() { let _ = parsed.arena.arena.expr(identity.expression); }
        checked.solved.validate().unwrap();
        let source = "let values = [1]\nlet wrong = values.push(...{item: true})\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))), "{:?}", checked.diagnostics);
    }

    #[test]
    fn source_builtin_error_named_spreads_keep_original_record_fields() {
        let source = "let fields = {message: \"missing\", status: null}\nlet failure = ProcessError.NotFound(...fields)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        for identity in checked.solved.expressions.keys() { let _ = parsed.arena.arena.expr(identity.expression); }
        checked.solved.validate().unwrap();
        let source = "let failure = ProcessError.NotFound(...{message: 7, status: null})\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", checked.diagnostics);
    }

    #[test]
    fn source_error_fail_keeps_pure_construction_and_captured_callable_kind() {
        let source = "pure failed(message: Str) -> Result[Unit] { error.fail(message: message) }\npure captured(message: Str) -> Result[Unit] { let failure = error.fail; failure(message) }\nlet first = failed(\"direct\")\nlet second = captured(\"captured\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.registry_references.len(), 1);
        assert!(checked.solved.operations.values().any(|operation| {
            let crate::sema::inference::RequirementTemplate::Operation { family, .. } = checked.solved.graph.requirement_template(operation.requirement).unwrap() else { return false; };
            checked.solved.graph.family(family).unwrap().iter().all(|candidate| matches!(checked.solved.operation_catalog.candidate(&checked.solved.graph, *candidate).unwrap(),
                super::super::operation_catalog::SolvedOperationAuthority::Registry(metadata) if metadata.public_label.as_str().as_str() == "error.fail" && metadata.kind == crate::sema::inference::CallableKind::Pure))
        }));
        checked.solved.validate().unwrap();
        for source in ["pure bad() { error.fail(message: 7) }\n", "pure bad() { let failure = error.fail; failure(7) }\n", "proc bad() [] { error.fail(\"message\")? }\n"] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))), "{source}: {:?}", checked.diagnostics);
        }
    }

    #[test]
    fn source_native_references_keep_independent_aliases_and_execution_permissions() {
        let source = "let failure = error.fail\nlet alias = failure\nlet first: Result[Unit] = alias(\"first\")\nlet second: Result[Unit] = alias(message: \"second\")\nlet clock = time.now\nproc timed() [time] -> Int { clock() }\nlet unused = time.now\nlet command = process.command_argv\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.registry_references.len(), 4);
        for reference in checked.solved.registry_references.values() {
            for candidate in reference.candidates(&checked.solved.graph).unwrap() {
                assert!(matches!(checked.solved.operation_catalog.candidate(&checked.solved.graph, candidate).unwrap(), super::super::SolvedOperationAuthority::Registry(_)));
            }
        }
        checked.solved.validate().unwrap();
        for source in [
            "let clock = time.now\npure forbidden() { clock() }\n",
            "let clock = time.now\nproc forbidden() [] { clock() }\n",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))), "{source}: {:?}", checked.diagnostics);
        }
    }

    #[test]
    fn source_native_singleton_protocols_retain_all_canonical_runtime_contracts() {
        use crate::sema::inference::{Atom, InferenceContext, TypeNode};
        use crate::sema::registry_graph::{RegistryGraph, RegistryProducerTransferPlan};
        use crate::modules::signature::{ApiArgCheck, SemanticRule};
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut registry = RegistryGraph::default();
        let span = crate::source::Span::new(SourceId::new(0), 0, 1);
        let reason = graph.reason(span, None).unwrap(); let string = graph.atom(Atom::Str).unwrap();
        let mut imported = 0;
        for (module, signatures) in super::super::api_spec().module_entries() {
            for entry in &signatures.functions {
                let family = registry.module_family(&mut graph, module, entry.name, span).unwrap();
                let [candidate] = graph.family(family).unwrap() else { continue; };
                let candidate = *candidate;
                let metadata = registry.metadata(&graph, candidate).unwrap().clone();
                let template = graph.candidate(candidate).unwrap();
                let protocol = !template.actual_eligibility.is_empty() || !template.effect_roles.is_empty()
                    || !template.output_effect_roles.is_empty() || !matches!(metadata.producer_transfer, RegistryProducerTransferPlan::Empty);
                if !protocol || metadata.semantic_rule != SemanticRule::Standard
                    || !matches!(metadata.argument_check, ApiArgCheck::Standard | ApiArgCheck::JsonCompatible) { continue; }
                let instance = graph.instantiate(metadata.scheme, 1, reason).unwrap();
                for substitution in &instance.substitutions { graph.unify(*substitution, string, reason).unwrap(); }
                graph.solve().unwrap();
                let TypeNode::Arrow(arrow) = graph.node(instance.ty).unwrap() else { panic!("module entry must retain a callable signature") };
                let mut parameters = Vec::new(); let mut arguments = Vec::new(); let mut declarations = Vec::new();
                for (slot, parameter) in arrow.params.iter().enumerate() {
                    parameters.push(format!("argument_{slot}: {}", type_source(&graph.export_type(parameter.ty).unwrap(), &mut declarations)));
                    arguments.push(format!("{}: argument_{slot}", parameter.label));
                }
                let result = graph.export_type(arrow.result).unwrap();
                let result_annotation = if contains_dynamic_module(&result) { String::new() } else { format!(" -> {}", type_source(&result, &mut declarations)) };
                let source = format!("{}proc invoke_{}({}) [fs,net,process,time,env,io,error]{result_annotation} {{ let native = {module}.{}; native({}) }}\n",
                    declarations.join(""), imported, parameters.join(", "), entry.name, arguments.join(", "));
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{}: {source} {:?}", metadata.public_label, parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source); drop(parsed);
                assert!(checked.diagnostics.is_empty(), "{}: {source} {:?}", metadata.public_label, checked.diagnostics);
                let reference = checked.solved.registry_references.values().next().expect("native reference retains its original contract");
                assert!(reference.native_authority().is_some(), "{} retains a native invocation protocol", metadata.public_label);
                assert_eq!(reference.candidates(&checked.solved.graph).unwrap().len(), 1);
                assert!(checked.solved.invocations.values().any(|invocation| {
                    checked.solved.graph.invocation_evidence(invocation.requirement).unwrap().is_some_and(|evidence| evidence.native_alternatives.iter().any(|alternative| {
                        checked.solved.graph.candidate_evidence(alternative.operation).unwrap().is_some_and(|selected| matches!(checked.solved.operation_catalog.candidate(&checked.solved.graph, selected.candidate).unwrap(),
                            super::super::SolvedOperationAuthority::Registry(actual) if actual.operation == metadata.operation))
                    }))
                }), "{} retains a selected canonical invocation receipt", metadata.public_label);
                checked.solved.validate().unwrap(); imported += 1;
            }
        }
        assert_eq!(imported, 36);
    }

    #[test]
    fn source_native_singleton_json_guards_keep_actual_values_and_execution_permissions() {
        for (declaration, reference, call, accepted, rejected) in [
            ("pure encoded(callback, value)", "json.encode_lines", "callback(values: value)", "[{valid: true}]", "[Path(\"item\")]"),
            ("proc encoded(callback, value) [fs]", "json.write", "callback(value: value, path: Path(\"target\"))", "{valid: true}", "Path(\"item\")"),
            ("proc encoded(callback, value) [fs]", "json.write_lines", "callback(values: value, path: Path(\"target\"))", "[{valid: true}]", "[Path(\"item\")]"),
            ("pure encoded(callback, value)", "json.set", "callback(replacement: true, path: [\"valid\"], value: value)", "{valid: true}", "Path(\"item\")"),
        ] {
            let definitions = format!("{declaration} {{ {call} }}\nlet native = {reference}\n");
            let source = format!("{definitions}let output = encoded(native, {accepted})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source); drop(parsed);
            assert!(checked.diagnostics.is_empty(), "{reference}: {:?}", checked.diagnostics);
            checked.solved.validate().unwrap();
            let source = format!("{definitions}let output = encoded(native, {rejected})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))),
                "{reference} must guard its original actual: {:?}", checked.diagnostics);
        }
        let source = "let write = json.write\nproc forbidden(value) [] { write(value: value, path: Path(\"target\")) }\nlet output = forbidden({valid: true})\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics);
    }

    #[test]
    fn source_native_reference_aliases_keep_fresh_schema_guards() {
        let source = "let empty = map.empty\nlet alias = empty\nlet numbers: Map[Int] = alias()\nlet words: Map[Str] = alias()\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.registry_references.len(), 1);
        let identity = *checked.solved.registry_references.keys().next().unwrap();
        assert_eq!(checked.solved.expression_callables[&identity].scheme, checked.solved.expression_schemes.get(&identity).copied());
        let reference = checked.solved.registry_references.values().next().unwrap();
        let super::RegistryReferenceContract::Arrow(reference) = &reference.contract else { panic!("unguarded native arrow keeps its original instance receipt"); };
        assert!(!reference.instantiated_requirements.is_empty());
        assert_eq!(reference.requirement_origins.len(), reference.instantiated_requirements.len());
        checked.solved.validate().unwrap();
        let source = "let empty = map.empty\nlet wrong: Map[List[Int], Int] = empty()\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))), "{:?}", checked.diagnostics);
    }

    #[test]
    fn published_native_reference_rejects_changed_substitutions_and_missing_guards() {
        for erase_guards in [false, true] {
            let source = "let empty = map.empty\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let mut checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let solved = std::sync::Arc::get_mut(&mut checked.solved).unwrap();
            let reference = solved.registry_references.values_mut().next().unwrap();
            let super::RegistryReferenceContract::Arrow(reference) = &mut reference.contract else { panic!("unguarded native arrow keeps its original instance receipt"); };
            assert_eq!(reference.substitutions.len(), 2);
            assert!(!reference.instantiated_requirements.is_empty());
            if erase_guards {
                reference.instantiated_requirements.clear();
                reference.requirement_origins.clear();
            } else { reference.substitutions.swap(0, 1); }
            assert!(solved.validate().is_err(), "a native reference must match the complete canonical scheme instance");
        }
    }

    #[test]
    fn source_process_command_argv_retains_original_types_and_native_authority() {
        for (declaration, call, expected_target) in [
            ("pure recipe(target: Str, argv: List[Str]) -> Command", "process.command_argv(target: target, argv: argv)", crate::sema::types::Type::Str),
            ("pure recipe(target: Path, argv: List[Path]) -> Command", "process.command_argv(target: target, argv: argv)", crate::sema::types::Type::Path),
            ("pure recipe(target: Path) -> Command", "process.command_argv(target, [\"child\", Path(\"item\")])", crate::sema::types::Type::Path),
            ("pure recipe(target: Any, argv: Any) -> Command", "process.command_argv(argv: argv, target: target)", crate::sema::types::Type::Any),
        ] {
            let source = format!("{declaration} {{ {call} }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let operations: Vec<_> = checked.solved.operations.values().filter(|operation| {
                let evidence = checked.solved.graph.candidate_evidence(operation.requirement).unwrap().unwrap();
                matches!(checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap(),
                    super::super::operation_catalog::SolvedOperationAuthority::Registry(metadata) if metadata.operation == crate::modules::RuntimeOp::ProcessCommandArgv)
            }).collect();
            assert_eq!(operations.len(), 1, "native factory must publish its canonical source certificate: {source}");
            let operation = operations[0];
            let target_argument = operation.binding.supplied_slots.iter().position(|slot| *slot == 0).unwrap();
            assert_eq!(checked.solved.graph.export_type(operation.actual_arguments[target_argument]).unwrap(), expected_target);
            let evidence = checked.solved.graph.candidate_evidence(operation.requirement).unwrap().unwrap();
            assert!(matches!(checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap(),
                super::super::operation_catalog::SolvedOperationAuthority::Registry(metadata) if metadata.operation == crate::modules::RuntimeOp::ProcessCommandArgv));
            let boundary = checked.solved.registry_boundaries.values().find(|boundary| matches!(boundary.kind, super::RegistryBoundaryKind::CommandArguments { .. })).unwrap();
            let super::RegistryBoundaryKind::CommandArguments { argv_children } = &boundary.kind else { panic!() };
            if call.contains("Path(\"item\")") {
                assert_eq!(argv_children.len(), 2);
                assert_eq!(argv_children.iter().map(|child| checked.solved.graph.export_type(child.actual).unwrap()).collect::<Vec<_>>(), vec![crate::sema::types::Type::Str, crate::sema::types::Type::Path]);
            } else { assert!(argv_children.is_empty()); }
            checked.solved.validate().unwrap();
        }
        for (source, code) in [
            ("let command = process.command_argv(\"child\", [])\n", "check.process-argv-empty"),
            ("let command = process.command_argv(\"child\", [\"child\", {bad: true}])\n", "check.type-mismatch"),
            ("let command = process.command_argv(\"child\", [\"child\"], cpu_max: 0)\n", "check.named-arg"),
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some(code)), "{source}: {:?}", checked.diagnostics);
        }
    }

    #[test]
    fn source_command_argv_children_keep_guards_through_generic_forwarding() {
        let definitions = "pure recipe(item, tail) { process.command_argv(\"child\", [\"child\", item, @tail]) }\npure forwarded(item, tail) { recipe(item, tail) }\n";
        let source = format!("{definitions}let first = forwarded(Path(\"item\"), [Path(\"other\")])\nlet second = forwarded(\"item\", [\"other\"])\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        let boundary = checked.solved.registry_boundaries.values().find(|boundary| matches!(boundary.kind, super::RegistryBoundaryKind::CommandArguments { .. })).unwrap();
        let super::RegistryBoundaryKind::CommandArguments { argv_children } = &boundary.kind else { panic!() };
        assert_eq!(argv_children.len(), 3);
        assert_eq!(argv_children.iter().map(|child| child.splice).collect::<Vec<_>>(), vec![false, false, true]);
        assert!(matches!(checked.solved.graph.node(checked.solved.graph.resolved(argv_children[1].actual).unwrap()).unwrap(), crate::sema::inference::TypeNode::Rigid { .. }));
        let crate::sema::inference::TypeNode::List(item) = checked.solved.graph.node(checked.solved.graph.resolved(argv_children[2].actual).unwrap()).unwrap() else { panic!("splice shape must remain an independently quantified List item"); };
        assert!(matches!(checked.solved.graph.node(checked.solved.graph.resolved(*item).unwrap()).unwrap(), crate::sema::inference::TypeNode::Rigid { .. }));
        for child in argv_children { assert_eq!(checked.solved.graph.resolved(child.actual).unwrap(), checked.solved.graph.resolved(checked.solved.expressions[&child.source]).unwrap()); }
        checked.solved.validate().unwrap();
        for invocation in ["forwarded(true, [\"other\"])", "forwarded(\"item\", [7])"] {
            let source = format!("{definitions}let wrong = {invocation}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))), "{source}: {:?}", checked.diagnostics);
        }
    }

    #[test]
    fn source_every_registered_module_retains_its_canonical_operation_authority() {
        use crate::sema::inference::{Atom, InferenceContext, TypeNode};
        use crate::sema::registry_graph::{RegistryGraph, RegistryOwner};
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut registry = RegistryGraph::default();
        let span = crate::source::Span::new(SourceId::new(0), 0, 1);
        let mut candidates = std::collections::BTreeSet::new();
        for (module, signatures) in super::super::api_spec().module_entries() {
            for entry in &signatures.functions {
                let family = registry.module_family(&mut graph, module, entry.name, span).unwrap();
                candidates.extend(graph.family(family).unwrap().iter().copied());
            }
        }
        assert_eq!(candidates.len(), if cfg!(feature = "native-tests") { 323 } else { 309 });
        let reason = graph.reason(span, None).unwrap(); let string = graph.atom(Atom::Str).unwrap();
        for (index, candidate) in candidates.into_iter().enumerate() {
            let metadata = registry.metadata(&graph, candidate).unwrap().clone();
            let RegistryOwner::Module(module) = metadata.owner else { panic!("not a module signature") };
            let instance = graph.instantiate(metadata.scheme, 1, reason).unwrap();
            for substitution in &instance.substitutions { graph.unify(*substitution, string, reason).unwrap(); }
            graph.solve().unwrap();
            let TypeNode::Arrow(arrow) = graph.node(instance.ty).unwrap() else { panic!("registry module entry is not callable") };
            let mut parameters = Vec::new(); let mut arguments = Vec::new(); let mut declarations = Vec::new();
            for (slot, parameter) in arrow.params.iter().enumerate() {
                let ty = graph.export_type(parameter.ty).unwrap();
                parameters.push(format!("argument_{slot}: {}", type_source(&ty, &mut declarations)));
                arguments.push(format!("{}: argument_{slot}", parameter.label));
            }
            let result_type = graph.export_type(arrow.result).unwrap();
            let result = if contains_dynamic_module(&result_type) { String::new() } else { format!(" -> {}", type_source(&result_type, &mut declarations)) };
            let declarations = declarations.join("");
            let source = format!("{declarations}proc module_{index}({}) [fs,net,process,time,env,io,error]{result} {{ {module}.{}({}) }}\n", parameters.join(", "), metadata.entry, arguments.join(", "));
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{}: {source} {:?}", metadata.public_label, parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{}: {source} {:?}", metadata.public_label, checked.diagnostics);
            assert_eq!(checked.solved.operations.len(), 1, "{}: {source}", metadata.public_label);
            let operation = checked.solved.operations.values().next().unwrap();
            let evidence = checked.solved.graph.candidate_evidence(operation.requirement).unwrap().unwrap();
            let authority = checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap();
            assert!(matches!(authority, super::super::operation_catalog::SolvedOperationAuthority::Registry(actual) if actual.operation == metadata.operation));
            if contains_dynamic_module(&result_type) {
                let declaration = checked.solved.declarations.values().next().unwrap();
                let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!() };
                assert_eq!(checked.solved.graph.export_type(signature.result).unwrap(), result_type);
            }
            checked.solved.validate().unwrap();
            let extra = if arguments.is_empty() { "unexpected: 1".to_string() } else { format!("{}, unexpected: 1", arguments.join(", ")) };
            let invalid = format!("{declarations}proc module_{index}({}) [fs,net,process,time,env,io,error]{result} {{ {module}.{}({extra}) }}\n", parameters.join(", "), metadata.entry);
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &invalid);
            assert!(parsed.diagnostics.is_empty(), "{invalid} {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &invalid);
            assert!(checked.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code.as_deref(), Some("check.arity" | "check.named-arg"))), "{} accepted an extra argument: {:?}", metadata.public_label, checked.diagnostics);
        }
    }

    #[test]
    fn source_cli_descriptor_retains_the_prepared_boundary_without_changing_the_candidate() {
        let source = "const schema = {count: {kind: \"Int\", default: 7}}\nlet parsed = cli.parse(schema: schema, argv: [])\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let _symbols = checked.solved.symbol_owner().enter();
        let boundary = checked.solved.registry_boundaries.values().next().unwrap_or_else(|| panic!("descriptor result retains its checked boundary; canonical operations: {}", checked.solved.operations.len()));
        let super::RegistryBoundaryKind::CliDescriptor { operation, plan } = &boundary.kind else { panic!("wrong boundary authority") };
        assert_eq!(*operation, crate::modules::RuntimeOp::CliParse);
        assert!(plan.matches_operation(*operation));
        assert_eq!(checked.solved.graph.export_type(boundary.result).unwrap(), plan.return_type(false));
        let requirement = boundary.requirement.unwrap();
        let crate::sema::inference::RequirementTemplate::Operation { call, .. } = checked.solved.graph.requirement_template(requirement).unwrap() else { panic!("missing canonical operation") };
        assert_eq!(checked.solved.graph.operation_call(call).unwrap().result, boundary.input);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn source_all_cli_descriptor_candidates_keep_their_checked_plan_authority() {
        use crate::modules::RuntimeOp;
        for (declarations, call, operation) in [
            ("const schema = {count: {kind: \"Int\", default: 7}}\n", "cli.parse(argv: [], schema: schema)", RuntimeOp::CliParse),
            ("const schema = {count: {kind: \"Int\", default: 7}}\n", "cli.parse_full(argv: [], schema: schema)", RuntimeOp::CliParseFull),
            ("const schema = {count: {kind: \"Int\", default: 7}}\n", "cli.applet(argv: [], schema: schema)", RuntimeOp::CliApplet),
            ("const commands = {build: {positionals: [\"root\"], types: {root: \"Path\"}}}\n", "cli.commands(commands: commands, argv: [\"build\", \"workspace\"])", RuntimeOp::CliCommands),
            ("const commands = {build: {positionals: [\"root\"], types: {root: \"Path\"}}}\nconst fallback = {positionals: [\"root\"], types: {root: \"Path\"}}\n", "cli.commands(fallback_command: fallback, commands: commands, rootless_default: \"build\", argv: [\"workspace\"])", RuntimeOp::CliCommands),
        ] {
            let source = format!("{declarations}let output = {call}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{call}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{call}: {:?}", checked.diagnostics);
            assert_eq!(checked.solved.operations.len(), 1);
            assert_eq!(checked.solved.registry_boundaries.len(), 1);
            let boundary = checked.solved.registry_boundaries.values().next().unwrap();
            let super::RegistryBoundaryKind::CliDescriptor { operation: selected, plan } = &boundary.kind else { panic!("missing descriptor authority") };
            assert_eq!(*selected, operation);
            assert!(plan.matches_operation(operation));
            let _symbols = checked.solved.symbol_owner().enter();
            assert_eq!(checked.solved.graph.export_type(boundary.result).unwrap(), plan.return_type(operation == RuntimeOp::CliParseFull));
            checked.solved.validate().unwrap();
        }
        let source = "const schema = {count: {kind: \"UnknownKind\"}}\nlet output = cli.parse([], schema)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.cli-descriptor")), "{:?}", checked.diagnostics);
    }

    #[test]
    fn source_dynamic_cli_commands_preserve_the_declared_result_carrier() {
        let source = "proc dynamic(commands: Record) [error] -> Result[Record] { cli.commands([], commands) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.operations.len(), 1);
        assert!(checked.solved.registry_boundaries.is_empty(), "dynamic descriptors cannot certify static fields");
        let operation = checked.solved.operations.values().next().unwrap();
        assert!(matches!(checked.solved.graph.export_type(operation.result), Ok(crate::sema::types::Type::Result(_, _))));
        checked.solved.validate().unwrap();
    }

    #[test]
    fn source_native_ground_families_keep_data_domains_and_creation_permissions() {
        for reference in ["fs.write", "fs.write_atomic"] {
            let definitions = format!("proc write_with(callback, file, data) [fs] {{ callback(path: file, data: data) }}\nlet writer = {reference}\n");
            let source = format!("{definitions}proc both(file: Path) [fs] -> Result[Unit] {{ let _ = write_with(writer, file, \"text\"); write_with(writer, file, b\"bytes\") }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source); drop(parsed);
            assert!(checked.diagnostics.is_empty(), "{reference}: {:?}", checked.diagnostics);
            let reference = checked.solved.registry_references.values().next().unwrap();
            assert_eq!(reference.candidates(&checked.solved.graph).unwrap().len(), 2);
            checked.solved.validate().unwrap();
            for body in ["proc bad(file: Path) [fs] { write_with(writer, file, 7) }", "proc bad(file: Path) [] { writer(path: file, data: \"text\") }"] {
                let source = format!("{definitions}{body}\n");
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))), "{source}: {:?}", checked.diagnostics);
            }
        }
    }

    #[test]
    fn source_native_descriptor_references_require_their_original_preparation_plans() {
        for source in ["let parser = cli.parse\n", "let parser = cli.parse_full\n", "let parser = cli.applet\n", "let parser = cli.commands\n"] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-relationship")),
                "{source} must retain its descriptor preparation plan: {:?}", checked.diagnostics);
        }
    }

    #[test]
    fn source_hash_reference_requires_its_algorithm_binding_authority() {
        let source = "let verify = hash.verify_file\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-relationship")),
            "a checksum callable must retain its original algorithm selector before it can be referenced: {:?}", checked.diagnostics);
    }

    #[test]
    fn source_hash_algorithm_alias_keeps_the_written_selector_and_canonical_slot() {
        for algorithm in ["md5", "sha1", "sha256", "sha512"] {
            let source = format!("proc verified(file: Path) [fs] {{ hash.verify_file(file, {algorithm}: \"checksum\") }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let boundary = checked.solved.registry_boundaries.values().next().expect("checksum selector retains its boundary");
            let super::RegistryBoundaryKind::HashAlgorithm { algorithm: selected } = boundary.kind else { panic!("wrong boundary authority") };
            assert_eq!(selected.as_str().as_str(), algorithm);
            assert_eq!(checked.solved.operations.values().next().unwrap().binding.supplied_slots, [0, 1]);
            checked.solved.validate().unwrap();
        }
    }

    #[test]
    fn source_every_builtin_schema_retains_explicit_validation_authority() {
        let schemas = xsh_registry::records::record_schemas();
        assert_eq!(schemas.len(), 73);
        let source = schemas.keys().enumerate().map(|(index, name)|
            format!("pure validate_schema_{index}(raw: Any) -> Result[{name}] {{ raw.require({name}) }}\n")
        ).collect::<String>();
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.registry_boundaries.len(), 73);
        drop(parsed);
        let _symbols = checked.solved.symbol_owner().enter();
        for (name, declared) in schemas {
            let authority = crate::symbol::Name::intern(format!("registry.schema.{name}"));
            let schema = checked.solved.operation_catalog.schema(&checked.solved.graph, authority).unwrap();
            assert_eq!(checked.solved.graph.export_type(schema.shape).unwrap(), crate::modules::signature::convert_type(&declared));
        }
        checked.solved.validate().unwrap();

        let source = "pure unchecked(raw: Any) -> EnvEntry { raw }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.dynamic-boundary")), "{:?}", checked.diagnostics);
    }

    #[test]
    fn source_builtin_error_constructor_retains_its_canonical_payload_authority() {
        let source = "pure failed(message) { AssertionError.Failed(message: message) }\npure forwarded(message) { failed(message) }\nlet first: AssertionError = forwarded(\"first\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        drop(parsed);
        let _symbols = checked.solved.symbol_owner().enter();
        assert!(checked.solved.operation_catalog.scoped_roots(&checked.solved.graph).unwrap().iter().any(|root|
            matches!(checked.solved.graph.export_type(root.ty), Ok(crate::sema::types::Type::ErrorVariant { family, variant })
                if family == crate::symbol::Name::intern("AssertionError") && variant == crate::symbol::Name::intern("Failed"))
        ), "builtin constructor must retain its declaring catalog authority");
        checked.solved.validate().unwrap();

        let source = "pure failed(message) { AssertionError.Failed(message: message) }\nlet invalid = failed(7)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))), "{:?}", checked.diagnostics);
    }

    #[test]
    fn source_every_builtin_error_variant_retains_its_declaring_authority() {
        let families = xsh_registry::errors::builtin_error_families();
        assert_eq!(families.iter().map(|family| family.variants.len()).sum::<usize>(), 17);
        let mut source = String::new();
        let mut index = 0;
        for family in &families {
            for variant in family.variants {
                let arguments = family.fields.iter().map(|field| match field.name {
                    "message" => "message: message", "status" => "status: null", _ => panic!("uncovered builtin payload field"),
                }).collect::<Vec<_>>().join(", ");
                source.push_str(&format!("pure builtin_error_{index}(message) {{ {}.{}({arguments}) }}\nlet error_{index} = builtin_error_{index}(\"message\")\n", family.name, variant.name));
                index += 1;
            }
        }
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        drop(parsed);
        let _symbols = checked.solved.symbol_owner().enter();
        for family in families {
            for variant in family.variants {
                let authority = crate::symbol::Name::intern(format!("registry.error.{}.{}", family.name, variant.name));
                let error = checked.solved.operation_catalog.error(&checked.solved.graph, authority).unwrap();
                assert_eq!(error.family.as_str().as_str(), family.name);
                assert_eq!(error.variant.as_str().as_str(), variant.name);
                assert_eq!(error.parameters.len(), family.fields.len());
                assert_eq!(error.facets.iter().map(|name| name.as_str().to_string()).collect::<Vec<_>>(), variant.facets);
            }
        }
        checked.solved.validate().unwrap();
    }

    #[test]
    fn source_all_json_boundary_calls_retain_actual_eligibility_and_permissions() {
        for (declaration, call, accepted, rejected) in [
            ("pure encoded(value)", "json.encode(value: value)", "{valid: true}", "Path(\"item\")"),
            ("pure encoded(value)", "json.encode_lines(values: value)", "[{valid: true}]", "[Path(\"item\")]"),
            ("proc encoded(value) [fs]", "json.write(value: value, path: \"target\")", "{valid: true}", "Path(\"item\")"),
            ("proc encoded(value) [fs]", "json.write_lines(values: value, path: \"target\")", "[{valid: true}]", "[Path(\"item\")]"),
            ("pure encoded(value)", "json.set(replacement: true, path: [\"valid\"], value: value)", "{valid: true}", "Path(\"item\")"),
        ] {
            let source = format!("{declaration} {{ {call} }}\nlet output = encoded({accepted})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{call}: {:?}", checked.diagnostics);
            assert_eq!(checked.solved.operations.len(), 1);
            checked.solved.validate().unwrap();
            let source = format!("{declaration} {{ {call} }}\nlet output = encoded({rejected})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))), "{call}: {:?}", checked.diagnostics);
        }
        let source = "proc written(value) [] { json.write(path: \"target\", value: value) }\nlet output = written({valid: true})\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))), "{:?}", checked.diagnostics);
    }

    #[test]
    fn source_user_and_contextual_schema_validation_retains_independent_input() {
        for expression in ["raw.require(Row)", "raw.require()?"] {
            let source = format!("type Row = {{name: Str}}\npure validated(raw: Any) -> Result[Row] {{ {expression} }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(31), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert_eq!(checked.solved.schema_validations.len(), 1, "each require expression retains a source-owned validation boundary");
            let (identity, boundary) = checked.solved.schema_validations.iter().next().unwrap();
            assert_eq!(identity.source, SourceId::new(31));
            assert_eq!(checked.solved.graph.export_type(boundary.input).unwrap(), Type::Any);
            drop(parsed);
            checked.solved.validate().unwrap();
        }
    }

    #[test]
    fn source_json_eligibility_is_retained_through_generic_forwarding() {
        let definitions = "pure encoded(value) { json.encode(value: value) }\npure forwarded(value) { encoded(value) }\n";
        let source = format!("{definitions}let output: Result[Str] = forwarded({{valid: true, count: 7}})\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.operations.len(), 1);
        for declaration in checked.solved.declarations.values() {
            assert_eq!(checked.solved.graph.scheme(declaration.scheme).unwrap().requirements.len(), 1);
        }
        checked.solved.validate().unwrap();

        let source = format!("{definitions}let output = forwarded(Path(\"item\"))\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))), "{:?}", checked.diagnostics);
    }
}
