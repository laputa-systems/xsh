use super::{CallBinding, Checker, SolvedCall, Type};
use crate::sema::inference::{CallableKind, EffectSummary, InferenceError, TypeId, TypeNode};
use crate::syntax::arena::{ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaProgram, ExprId};
use crate::source::Span;

impl Checker {
    pub(super) fn retain_graph_callable_receiver_owner(&mut self, arena: &ArenaProgram, value: ExprId, caller: Option<super::DeclarationIdentity>) -> bool {
        if !self.graph_generation { return true; }
        let receiver = self.expression_identity(arena, value);
        let previous = self.generic.borrow().facts.expression_owners.get(&receiver).copied();
        if previous.is_some() && previous != caller {
            self.error(arena.arena.expr(value).span, "callable receiver belongs to another lexical declaration", "check.callable-owner");
            return false;
        }
        if let Some(owner) = caller { self.generic.borrow_mut().facts.expression_owners.insert(receiver, owner); }
        true
    }

    pub(super) fn graph_callable_value(&mut self, arena: &ArenaProgram, source: &str, expression: ExprId) -> Option<Type> {
        let target = self.graph_callable_target(arena, expression)?;
        self.prepare_graph_callable_value(arena, source, expression);
        let target = self.graph_callable_target(arena, expression).unwrap_or(target);
        if !self.graph_generation { return Some(Type::Graph(target.signature)); }
        if self.principal_callable_initializer {
            if let Some(scheme) = target.scheme {
                let identity = self.expression_identity(arena, expression);
                self.generic.borrow_mut().facts.expression_schemes.insert(identity, scheme);
            }
            return Some(Type::Graph(target.signature));
        }
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let Some(scheme) = target.scheme else { return Ok(target.signature); };
            let reason = state.facts.graph.reason(arena.arena.expr(expression).span, None)?;
            let instance = state.facts.graph.instantiate(scheme, self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 }), reason)?;
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.extend(instance.requirements); }
            Ok::<_, InferenceError>(instance.ty)
        })();
        Some(match outcome { Ok(ty) => Type::Graph(ty), Err(error) => { self.graph_error(arena.arena.expr(expression).span, error); Type::Invalid } })
    }

    pub(super) fn is_graph_callable_value(&self, ty: &Type) -> bool {
        let Type::Graph(ty) = ty else { return false; };
        self.generic.borrow().facts.graph.callable_signature(*ty).is_ok()
    }

    pub(super) fn join_graph_callable_values(&mut self, left: &Type, right: &Type, span: Span) -> Option<Type> {
        let (Type::Graph(left), Type::Graph(right)) = (left, right) else { return None; };
        {
            let state = self.generic.borrow();
            if state.facts.graph.callable_signature(*left).is_err() || state.facts.graph.callable_signature(*right).is_err() { return None; }
        }
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let reason = state.facts.graph.reason(span, None)?;
            let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
            state.facts.graph.join_callable_values(*left, *right, level, reason)
        })();
        Some(match outcome { Ok(ty) => Type::Graph(ty), Err(error) => { self.graph_error(span, error); Type::Invalid } })
    }

    pub(super) fn graph_computed_call(&mut self, arena: &ArenaProgram, source: &str, callee: ExprId, args: &[ArenaCallArg], span: Span) -> Option<Type> {
        let value = match arena.arena.expr(callee).kind {
            ArenaExprKind::Ident(name) if self.lookup(name).is_some_and(|binding| binding.ty.contains_graph()) => callee,
            ArenaExprKind::Field { base, name } if name == "call" => {
                if let ArenaExprKind::Ident(name) = arena.arena.expr(base).kind
                    && self.lookup(name).is_none() { return None; }
                base
            }
            ArenaExprKind::Field { base, name } => {
                let base_type = match arena.arena.expr(base).kind {
                    ArenaExprKind::Ident(name) => {
                        let binding = self.lookup(name)?;
                        if binding.static_namespace.is_some() { return None; }
                        binding.ty.clone()
                    }
                    _ => self.expr_types.get(&arena.arena.expr(base).span)?.clone(),
                };
                let field_type = match &base_type {
                    Type::Record(fields) => fields.get(&name).cloned(),
                    Type::Module(_) => return None,
                    Type::Graph(id) => {
                        let state = self.generic.borrow();
                        match state.facts.graph.resolved(*id).and_then(|id| state.facts.graph.node(id)) {
                            Ok(TypeNode::Record(row)) => state.facts.graph.row_data(*row).ok().and_then(|row| row.fields.iter().find(|field| field.label == name)).map(|field| Type::Graph(field.ty)),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                let Some(Type::Graph(field)) = field_type else { return None; };
                let state = self.generic.borrow();
                if state.facts.graph.callable_signature(field).is_err() { return None; }
                drop(state);
                callee
            }
            ArenaExprKind::Index { .. } | ArenaExprKind::If { .. } | ArenaExprKind::Call { .. } => callee,
            _ => return None,
        };
        let call_expression = self.current_expression?;
        let identity = self.expression_identity(arena, call_expression);
        if let Some(invocation) = self.generic.borrow().facts.invocations.get(&identity) {
            let state = self.generic.borrow();
            let Ok(crate::sema::inference::RequirementTemplate::CallableInvocation { call }) = state.facts.graph.requirement_template(invocation.requirement) else { return Some(Type::Invalid); };
            return Some(state.facts.graph.invocation_call(call).map(|call| self.graph_view(call.result)).unwrap_or(Type::Invalid));
        }
        if !self.graph_generation || self.generic.borrow().facts.calls.contains_key(&identity) {
            return self.generic.borrow().facts.calls.get(&identity).map(|call| {
                let state = self.generic.borrow();
                let Ok(TypeNode::Arrow(arrow)) = state.facts.graph.node(call.signature) else { return Type::Invalid; };
                self.graph_view(arrow.result)
            });
        }
        let caller = self.current_generic;
        let ty = self.check_expr_arena(arena, source, value, None);
        let Type::Graph(signature) = ty else { return None; };
        if !self.retain_graph_callable_receiver_owner(arena, value, caller) { return Some(Type::Invalid); }
        let arrow = {
            let state = self.generic.borrow();
            match state.facts.graph.resolved(signature).and_then(|id| state.facts.graph.node(id)) {
                Ok(TypeNode::Arrow(arrow)) => arrow.clone(),
                Ok(TypeNode::Meta(_) | TypeNode::Rigid { .. }) => {
                    drop(state);
                    return Some(self.check_graph_inferred_invocation(arena, source, signature, args, span));
                }
                Ok(TypeNode::NativeCallable(_) | TypeNode::CallableChoice(_)) => {
                    drop(state);
                    return Some(self.check_graph_inferred_invocation(arena, source, signature, args, span));
                }
                _ => return None,
            }
        };
        if self.in_pure && arrow.kind != CallableKind::Pure {
            self.error(span, "effectful proc is not allowed in pure functions", "check.pure-effect");
        }
        let outcome = (|| {
            let (actual_arguments, binding) = self.check_graph_callable_arguments(arena, source, signature, args, span, &[])?;
            {
                let mut state = self.generic.borrow_mut();
                state.facts.graph.solve()?;
            }
        if arrow.kind != CallableKind::Pure {
            self.record_graph_effect_summary(arrow.effects, span);
            let summary = self.generic.borrow().facts.graph.closed_effect_summary(arrow.effects);
            if let Ok(EffectSummary::Closed(bits)) = summary {
                for (bit, effect) in [(1, super::Effect::Fs), (2, super::Effect::Net), (4, super::Effect::Process), (8, super::Effect::Env), (16, super::Effect::Time), (32, super::Effect::Error), (64, super::Effect::Io)] {
                    if bits.0 & bit != 0 { self.require_effect(effect, span, "computed callable"); }
                }
            } else {
                self.record_effect_contract(&None, "computed callable");
                if self.current_effects.is_some() { self.error(span, "computed callable has an unknown effect contract", "check.effect-violation"); }
            }
            self.invalidate_mutable_narrowings();
        }
            let mut state = self.generic.borrow_mut();
            state.producer_inputs.call_bindings.insert(identity, binding.clone());
            state.facts.calls.insert(identity, SolvedCall { signature, declaration: None, caller: self.current_generic,
                argument_producers: vec![std::collections::BTreeMap::new(); actual_arguments.len()], result_producers: std::collections::BTreeMap::new(),
                result_producer_flow: None,
                requirements: Vec::new(), requirement_origins: Vec::new(), substitutions: Vec::new(), effect_substitutions: Vec::new(), actual_arguments, binding });
            state.facts.expressions.insert(identity, arrow.result);
            Ok::<_, InferenceError>(arrow.result)
        })();
        Some(match outcome { Ok(ty) => self.graph_view(ty), Err(error) => { self.graph_error(span, error); Type::Invalid } })
    }

    fn check_graph_inferred_invocation(&mut self, arena: &ArenaProgram, source: &str, callable: TypeId, args: &[ArenaCallArg], span: Span) -> Type {
        use crate::sema::arguments::{ArgumentValueSource, expand_named_arguments};
        use crate::sema::inference::{CallableDomain, EffectSet, InvocationArgument, InvocationArgumentKind, InvocationCall};
        let Some(expression) = self.current_expression else { self.graph_error(span, InferenceError::InvalidScheme); return Type::Invalid; };
        let identity = self.expression_identity(arena, expression);
        let outcome = (|| {
            let command_parameters = self.graph_command_callable_parameters(callable)?;
            let mut command_children = Vec::new();
            let mut occupied = command_parameters.as_ref().map(|parameters| vec![false; parameters.len()]);
            let mut next = 0;
            let mut values = std::collections::BTreeMap::new();
            for argument in args {
                let value = match argument.kind { ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. } | ArenaCallArgKind::NamedSpread { value, .. } | ArenaCallArgKind::Splice { value, .. } => value };
                let actual = if let (Some(parameters), Some(occupied)) = (&command_parameters, &mut occupied) {
                    let slot = match argument.kind {
                        ArenaCallArgKind::Named { name, .. } => parameters.iter().position(|label| *label == name),
                        ArenaCallArgKind::Positional(_) => { while next < occupied.len() && occupied[next] { next += 1; } let slot = (next < occupied.len()).then_some(next); next += 1; slot },
                        _ => None,
                    };
                    if let Some(slot) = slot { occupied[slot] = true; }
                    let (actual, children) = self.check_graph_command_argument(arena, source, value, slot);
                    command_children.extend(children); actual
                } else { self.check_expr_arena(arena, source, value, None) };
                values.insert(value, actual);
            }
            let expanded = expand_named_arguments(arena, args, |expression| values.get(&expression).cloned()).map_err(|error| {
                self.error(error.span, &error.message, "check.named-spread"); InferenceError::InvalidScheme
            })?;
            let mut arguments = Vec::with_capacity(expanded.len());
            for argument in &expanded {
                let kind = if matches!(argument.value, ArgumentValueSource::PositionalSplice(_)) { InvocationArgumentKind::PositionalSplice }
                    else if let Some(name) = argument.name { InvocationArgumentKind::Named(name) } else { InvocationArgumentKind::Positional };
                let checked = self.graph_type(&argument.ty, argument.span)?;
                let original = match argument.value {
                    ArgumentValueSource::Expression(expression) | ArgumentValueSource::PositionalSplice(expression) => {
                        self.generic.borrow().facts.expressions.get(&self.expression_identity(arena, expression)).copied()
                    }
                    ArgumentValueSource::RecordField { .. } => None,
                };
                let ty = if let Some(original) = original {
                    let mut state = self.generic.borrow_mut();
                    let reason = state.facts.graph.reason(argument.span, None)?;
                    state.facts.graph.unify(original, checked, reason)?;
                    original
                } else { checked };
                arguments.push(InvocationArgument { kind, ty });
            }
            self.record_argument_sources(identity, &expanded)?;
            let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
            let mut state = self.generic.borrow_mut();
            let result = state.facts.graph.fresh(level, span)?;
            let effects = if self.in_pure { EffectSummary::Closed(EffectSet::EMPTY) } else { EffectSummary::Variable(state.facts.graph.fresh_derived_effect_at(level, None)?) };
            let reason = state.facts.graph.reason(span, None)?;
            let requirement = state.facts.graph.require_callable_invocation(InvocationCall { callable, arguments, result, effects,
                domain: if self.in_pure { CallableDomain::Pure } else { CallableDomain::AnyCallable } }, reason)?;
            state.facts.graph.solve()?;
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.push(requirement); state.facts.expression_owners.insert(identity, owner); }
            state.facts.invocations.insert(identity, super::SolvedInvocation { requirement, caller: self.current_generic });
            state.producer_inputs.invocation_requirements.insert(identity, requirement);
            state.facts.expressions.insert(identity, result);
            let command_requirement = if command_parameters.is_some() {
                Some(state.facts.graph.native_invocation_children(requirement)?.first().ok_or(InferenceError::InvalidScheme)?.operation)
            } else { None };
            drop(state);
            if let Some(requirement) = command_requirement { self.record_graph_command_arguments(identity, requirement, command_children)?; }
            self.record_graph_effect_summary(effects, span);
            Ok::<_, InferenceError>(result)
        })();
        match outcome { Ok(result) => self.graph_view(result), Err(error) => { self.graph_error(span, error); Type::Invalid } }
    }

    pub(super) fn check_graph_callable_arguments(&mut self, arena: &ArenaProgram, source: &str, signature: TypeId, args: &[ArenaCallArg], span: Span, schemas: &[Option<crate::sema::constants::SchemaExpectation>]) -> Result<(Vec<TypeId>, CallBinding), InferenceError> {
        use crate::sema::arguments::{ArgumentValueSource, expand_named_arguments};
        use crate::sema::inference::{InvocationArgument, InvocationArgumentKind, InvocationArgumentSegment, InvocationCall, CallableDomain, InvocationPlanError, InvocationProblem};
        let arrow = {
            let state = self.generic.borrow();
            let TypeNode::Arrow(arrow) = state.facts.graph.node(state.facts.graph.resolved(signature)?)? else { return Err(InferenceError::KindMismatch); };
            arrow.clone()
        };
        let spreading = args.iter().any(|arg| matches!(arg.kind, ArenaCallArgKind::NamedSpread { .. }));
        let mut values = std::collections::BTreeMap::new();
        if spreading {
            for arg in args {
                let expression = match arg.kind { ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. } | ArenaCallArgKind::NamedSpread { value, .. } | ArenaCallArgKind::Splice { value, .. } => value };
                let ty = self.check_expr_arena(arena, source, expression, None);
                let ty = if let Type::Graph(id) = ty {
                    let state = self.generic.borrow();
                    match state.facts.graph.node(state.facts.graph.resolved(id)?)? {
                        TypeNode::Record(row) if state.facts.graph.row_data(*row)?.tail.is_none() => Type::Record(state.facts.graph.row_data(*row)?.fields.iter().map(|field| (field.label, self.graph_view(field.ty))).collect()),
                        _ => Type::Graph(id),
                    }
                } else { ty };
                values.insert(expression, ty);
            }
        }
        let expanded = expand_named_arguments(arena, args, |id| values.get(&id).cloned()).map_err(|error| {
            self.error(error.span, &error.message, "check.named-spread");
            InferenceError::InvalidScheme
        })?;
        let kinds: Vec<_> = expanded.iter().map(|argument| match argument.value {
            ArgumentValueSource::PositionalSplice(_) => InvocationArgumentKind::PositionalSplice,
            _ => argument.name.map(InvocationArgumentKind::Named).unwrap_or(InvocationArgumentKind::Positional),
        }).collect();
        let binding = self.generic.borrow_mut().facts.graph.plan_invocation_arguments(signature, &kinds);
        let binding = match binding {
            Ok(binding) => binding,
            Err(InvocationPlanError::Graph(error)) => return Err(error),
            Err(InvocationPlanError::Binding(problem)) => {
                let (message, code) = match problem {
                    InvocationProblem::MissingArgument(name) => (format!("missing required parameter `{name}`"), "check.arity"),
                    InvocationProblem::TooManyArguments => ("too many positional arguments".to_string(), "check.arity"),
                    InvocationProblem::UnknownLabel(name) => (format!("unknown named parameter `{name}`"), "check.named-arg"),
                    InvocationProblem::DuplicateArgument(name) => (format!("parameter `{name}` supplied more than once"), "check.named-arg"),
                    InvocationProblem::InvalidSplice => ("positional splice must retain a checked List type".to_string(), "check.call-splice"),
                    InvocationProblem::InvalidRest => ("rest parameter must retain a checked List type".to_string(), "check.call-splice"),
                    InvocationProblem::NotCallable | InvocationProblem::CallableKind => ("value does not have a permitted callable signature".to_string(), "check.call-target"),
                };
                self.graph_boundary_error(span, &message, code);
                return Err(InferenceError::InvalidScheme);
            }
        };
        let mut actual_arguments = Vec::with_capacity(expanded.len());
        for (index, argument) in expanded.iter().enumerate() {
            let slot = match &binding.dynamic {
                Some(dynamic) => dynamic.segments.iter().find_map(|segment| match segment {
                    InvocationArgumentSegment::StaticSlot { argument, slot } if *argument == index => Some(*slot),
                    _ => None,
                }),
                None => binding.supplied_slots.get(index).copied(),
            };
            let expected = slot.map(|slot| {
                if arrow.params[slot].rest && !matches!(argument.value, ArgumentValueSource::PositionalSplice(_)) {
                    let state = self.generic.borrow();
                    match state.facts.graph.node(state.facts.graph.resolved(arrow.params[slot].ty)?)? { TypeNode::List(item) => Ok(*item), _ => Err(InferenceError::KindMismatch) }
                } else { Ok(arrow.params[slot].ty) }
            }).transpose()?;
            let view = expected.map(|expected| self.graph_view(expected));
            let actual = if spreading { argument.ty.clone() } else {
                let (ArgumentValueSource::Expression(expression) | ArgumentValueSource::PositionalSplice(expression)) = argument.value else { return Err(InferenceError::InvalidScheme); };
                let schema = slot.and_then(|slot| schemas.get(slot).cloned().flatten().map(|schema| (slot, schema))).and_then(|(slot, schema)| {
                    if arrow.params[slot].rest && !matches!(argument.value, ArgumentValueSource::PositionalSplice(_)) { schema.children.get(&crate::sema::constants::SchemaComponent::Item).cloned() } else { Some(schema) }
                });
                self.graph_argument_depth += 1;
                let actual = self.check_expr_with_schema_arena(arena, source, crate::syntax::arena::ArenaExprOrRun::Expr(expression), view.as_ref(), schema);
                self.graph_argument_depth -= 1;
                actual
            };
            if let Some(view) = &view && self.current_generic.is_none() && actual.contains_inference() && !view.contains_graph() { self.expect_type(view, &actual, argument.span); }
            let checked = self.graph_type(&actual, argument.span)?;
            let actual = match argument.value {
                crate::sema::arguments::ArgumentValueSource::Expression(expression)
                | crate::sema::arguments::ArgumentValueSource::PositionalSplice(expression) => {
                    let identity = self.expression_identity(arena, expression);
                    let original = self.generic.borrow().facts.expressions.get(&identity).copied();
                    if let Some(original) = original {
                        let mut state = self.generic.borrow_mut();
                        let reason = state.facts.graph.reason(argument.span, None)?;
                        state.facts.graph.unify(original, checked, reason)?;
                        original
                    } else { checked }
                }
                crate::sema::arguments::ArgumentValueSource::RecordField { .. } => checked,
            };
            if let Some(expected) = expected && self.graph_argument_needs_validation(expected, actual)? { self.graph_boundary_error(argument.span, "unchecked dynamic argument needs validation before this callable boundary", "check.dynamic-boundary"); }
            let mut state = self.generic.borrow_mut();
            let reason = state.facts.graph.reason(argument.span, None)?;
            if let Some(expected) = expected { state.facts.graph.assignable(expected, actual, reason)?; }
            if let ArgumentValueSource::Expression(expression) | ArgumentValueSource::PositionalSplice(expression) = argument.value {
                let identity = self.expression_identity(arena, expression);
                state.facts.expressions.insert(identity, actual);
                if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(identity, owner); }
            }
            actual_arguments.push(actual);
        }
        if let Some(expression) = self.current_expression {
            let identity = self.expression_identity(arena, expression);
            self.record_argument_sources(identity, &expanded)?;
            let mut state = self.generic.borrow_mut();
            if binding.dynamic.is_some() || kinds.iter().any(|kind| matches!(kind, InvocationArgumentKind::PositionalSplice)) {
                let reason = state.facts.graph.reason(span, None)?;
                let requirement = state.facts.graph.require_callable_invocation(InvocationCall {
                    callable: signature,
                    arguments: kinds.iter().copied().zip(actual_arguments.iter().copied()).map(|(kind, ty)| InvocationArgument { kind, ty }).collect(),
                    result: arrow.result, effects: arrow.effects,
                    domain: if self.in_pure { CallableDomain::Pure } else { CallableDomain::AnyCallable },
                }, reason)?;
                if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.push(requirement); }
                state.facts.invocations.insert(identity, super::SolvedInvocation { requirement, caller: self.current_generic });
                state.producer_inputs.invocation_requirements.insert(identity, requirement);
            }
        }
        Ok((actual_arguments, CallBinding { supplied_slots: binding.supplied_slots, default_slots: binding.default_slots, rest_slot: binding.rest_slot, dynamic: binding.dynamic }))
    }
}
