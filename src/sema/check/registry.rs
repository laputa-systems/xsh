use super::{CallBinding, Checker, SolvedOperation, Type};
use crate::sema::inference::{CallableKind, EffectSummary, InferenceError, OperationCall, TypeNode};
use crate::syntax::arena::{ArenaCallArg, ArenaProgram};
use crate::source::Span;

impl Checker {
    pub(super) fn check_graph_module_call(
        &mut self, arena: &ArenaProgram, source: &str, module: &str, name: &str,
        args: &[ArenaCallArg], span: Span, expected: Option<&Type>,
    ) -> Type {
        self.check_graph_module_call_with_arguments(arena, source, module, name, args, span, expected, None)
    }

    pub(super) fn check_graph_module_call_with_checked_arguments(
        &mut self, arena: &ArenaProgram, source: &str, module: &str, name: &str,
        args: &[ArenaCallArg], span: Span, expected: Option<&Type>,
        checked: std::collections::BTreeMap<crate::syntax::arena::ExprId, Type>,
    ) -> Type {
        self.check_graph_module_call_with_arguments(arena, source, module, name, args, span, expected, Some(checked))
    }

    fn check_graph_module_call_with_arguments(
        &mut self, arena: &ArenaProgram, source: &str, module: &str, name: &str,
        args: &[ArenaCallArg], span: Span, expected: Option<&Type>,
        checked: Option<std::collections::BTreeMap<crate::syntax::arena::ExprId, Type>>,
    ) -> Type {
        let Some(expression) = self.current_expression else {
            self.graph_error(span, InferenceError::Boundary("registry call requires its source expression identity"));
            return Type::Invalid;
        };
        let identity = self.expression_identity(arena, expression);
        if !self.graph_generation || self.generic.borrow().facts.operations.contains_key(&identity) {
            return self.generic.borrow().facts.operations.get(&identity).map(|operation| self.graph_view(operation.result)).unwrap_or(Type::Invalid);
        }
        let outcome = (|| {
            let (family, shapes) = (|| {
                let mut state = self.generic.borrow_mut();
                let super::generic::GenericState { facts, registry, .. } = &mut *state;
                let family = registry.module_family(&mut facts.graph, module, name, span)?;
                let candidates = facts.graph.family(family)?.to_vec();
                let mut allowed = Vec::new();
                let mut shapes = Vec::new();
                for candidate in candidates {
                    let metadata = registry.metadata(&facts.graph, candidate)?;
                    if self.in_pure && metadata.kind != CallableKind::Pure { continue; }
                    shapes.push((candidate, metadata.parameters.iter().map(|parameter| (parameter.label, parameter.defaulted)).collect::<Vec<_>>()));
                    allowed.push(candidate);
                }
                if allowed.is_empty() {
                    drop(state);
                    self.error(span, "effectful module API is not allowed in pure functions", "check.pure-effect");
                    return Ok((None, Vec::new()));
                }
                let family = facts.graph.register_family(&allowed)?;
                Ok((Some(family), shapes))
            })()?;
            let Some(family) = family else { return Ok(Type::Invalid); };
            let supplied_checked = checked.is_some();
            let mut checked = checked.unwrap_or_default();
            for arg in args {
                let value = match arg.kind {
                    crate::syntax::arena::ArenaCallArgKind::Positional(value) | crate::syntax::arena::ArenaCallArgKind::Named { value, .. }
                    | crate::syntax::arena::ArenaCallArgKind::NamedSpread { value, .. } | crate::syntax::arena::ArenaCallArgKind::Splice { value, .. } => value,
                };
                if supplied_checked {
                    if !checked.contains_key(&value) { return Err(InferenceError::Boundary("canonical module argument lacks its checked source value")); }
                    continue;
                }
                let previous_schema = self.expected_schema.take();
                self.expected_schema = Some(crate::sema::constants::SchemaExpectation::default());
                let actual = self.check_expr_arena(arena, source, value, None);
                self.expected_schema = previous_schema;
                checked.insert(value, actual);
            }
            let expanded = match crate::sema::arguments::expand_named_arguments(arena, args, |expression| checked.get(&expression).cloned()) {
                Ok(expanded) => expanded,
                Err(error) => { self.error(error.span, &error.message, "check.named-spread"); return Ok(Type::Invalid); }
            };
            let mut viable = Vec::new();
            let mut binding = None;
            let mut parameter_count = None;
            let mut rejected = None;
            for (candidate, shape) in &shapes {
                let params = shape.iter().map(|&(name, defaulted)| crate::sema::types::CallableParamType { name, ty: Type::Invalid, defaulted, rest: false }).collect::<Vec<_>>();
                match crate::sema::arguments::bind_static_arguments(&params, &expanded) {
                    Ok(candidate_binding) => {
                        if binding.as_ref().is_some_and(|previous: &crate::sema::arguments::StaticArgumentBinding| previous.argument_slots != candidate_binding.argument_slots || previous.omitted_slots != candidate_binding.omitted_slots)
                            || parameter_count.is_some_and(|count| count != params.len()) {
                            return Err(InferenceError::Boundary("registry overloads require distinct checked source binding plans"));
                        }
                        parameter_count = Some(params.len());
                        binding = Some(candidate_binding);
                        viable.push(*candidate);
                    }
                    Err(error) => { rejected = Some((error, params)); }
                }
            }
            let Some(binding) = binding else {
                let (error, params) = rejected.ok_or(InferenceError::InvalidScheme)?;
                let required = params.iter().filter(|parameter| !parameter.defaulted).count();
                let code = if expanded.len() < required || expanded.len() > params.len() { "check.arity" } else { "check.named-arg" };
                self.error(if args.is_empty() { span } else { error.span }, &error.message, code);
                return Ok(Type::Invalid);
            };
            let family = if viable.len() == shapes.len() { family } else { self.generic.borrow_mut().facts.graph.register_family(&viable)? };
            let parameter_count = parameter_count.unwrap();
            let mut supplied = vec![None; parameter_count];
            let mut actual_arguments = Vec::new();
            for (argument, &slot) in expanded.iter().zip(&binding.argument_slots) {
                let actual = self.graph_type(&argument.ty, argument.span)?;
                supplied[slot] = Some(actual);
                actual_arguments.push(actual);
            }
            let expected = expected.map(|ty| self.graph_type(ty, span)).transpose()?;
            let mut state = self.generic.borrow_mut();
            let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
            let output_effect_bindings = {
                let super::generic::GenericState { facts, registry, .. } = &mut *state;
                registry.output_effect_bindings(&mut facts.graph, family, level)?
            };
            let graph = &mut state.facts.graph;
            let result = graph.fresh(level, span)?;
            let effects = EffectSummary::Variable(graph.fresh_execution_effect(None)?);
            let reason = graph.reason(span, None)?;
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: supplied, result, effects, effect_bindings: Vec::new(), output_effect_bindings: output_effect_bindings.clone() }, reason)?;
            graph.solve()?;
            if let Some(expected) = expected {
                let contextual_result = match (graph.node(graph.resolved(expected)?)?, graph.node(graph.resolved(result)?)?) {
                    (TypeNode::Result(_, _) | TypeNode::Meta(_) | TypeNode::Rigid { .. }, _) => result,
                    (_, TypeNode::Result(value, _)) => *value,
                    _ => result,
                };
                drop(state);
                if self.graph_argument_needs_validation(expected, contextual_result)? {
                    self.graph_boundary_error(span, "unchecked native result needs validation before this callable boundary", "check.dynamic-boundary");
                    return Ok(Type::Invalid);
                }
                state = self.generic.borrow_mut();
                state.facts.graph.assignable(expected, contextual_result, reason)?;
                state.facts.graph.solve()?;
            }
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.push(requirement); }
            state.facts.operations.insert(identity, SolvedOperation { requirement, result, effects, receiver: None, actual_arguments,
                argument_coercions: Vec::new(),
                binding: CallBinding { supplied_slots: binding.argument_slots.clone(), default_slots: binding.omitted_slots.clone(), rest_slot: None, dynamic: None }, caller: self.current_generic });
            state.facts.expressions.insert(identity, result);
            if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(identity, owner); }
            drop(state);
            self.record_graph_effect_summary(effects, span);
            self.record_registry_operation_producer_flow(arena, expression, requirement, None, &expanded, &binding, span)?;
            Ok(self.graph_view(result))
        })();
        match outcome {
            Ok(ty) => ty,
            Err(error) => {
                match self.registry_json_guard_rejected(&error) {
                    Ok(true) => self.error(span, "value is not JSON-compatible; convert Path, Bytes, Status, Result, and errors explicitly", "check.json-compatible"),
                    Ok(false) => self.graph_error(span, error),
                    Err(error) => self.graph_error(span, error),
                }
                Type::Invalid
            }
        }
    }
}
