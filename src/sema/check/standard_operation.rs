use super::{CallBinding, Checker, MethodReceiver, SolvedOperation, Type};
use crate::sema::inference::{CallableKind, EffectRole, EffectSummary, InferenceError, OperationCall};
use crate::source::Span;
use crate::syntax::arena::{ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaProgram};

impl Checker {
    pub(super) fn check_graph_standard_method(
        &mut self, arena: &ArenaProgram, source: &str, receiver_ty: &Type,
        receiver_expression: Option<crate::syntax::arena::ExprId>, receiver_path: &super::ProducerPath, receiver_kind: Option<MethodReceiver>, name: &str, args: &[ArenaCallArg],
        span: Span, expected: Option<&Type>,
        receiver_schema: Option<crate::sema::constants::SchemaExpectation>,
    ) -> Type {
        let Some(expression) = self.current_expression else {
            self.graph_error(span, InferenceError::Boundary("method requires its source expression identity"));
            return Type::Invalid;
        };
        let identity = self.expression_identity(arena, expression);
        if !self.graph_generation || self.generic.borrow().facts.operations.contains_key(&identity) {
            return self.generic.borrow().facts.operations.get(&identity).map(|operation| self.graph_view(operation.result)).unwrap_or(Type::Invalid);
        }
        let outcome = (|| {
            let receiver = if receiver_kind == Some(MethodReceiver::PathConstructor) { None } else { Some(self.graph_type(receiver_ty, span)?) };
            let mut checked = std::collections::BTreeMap::new();
            let schema_contexts = receiver_kind.and_then(|receiver| super::api_spec().method_overloads(receiver, name))
                .filter(|methods| methods.len() == 1)
                .map(|methods| crate::sema::builtin_templates::parameter_schema_contexts(&methods[0].sig, methods[0].receiver_ty.as_ref(), receiver_schema.as_ref()));
            for (index, argument) in args.iter().enumerate() {
                let value = match argument.kind { ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. }
                    | ArenaCallArgKind::NamedSpread { value, .. } | ArenaCallArgKind::Splice { value, .. } => value };
                let parameter_index = match argument.kind {
                    ArenaCallArgKind::Named { name: label, .. } => receiver_kind.and_then(|receiver| super::api_spec().method_overloads(receiver, name))
                        .and_then(|methods| methods.first()).and_then(|method| method.sig.params.iter().position(|parameter| crate::symbol::Name::intern(parameter.name) == label)).unwrap_or(index),
                    _ => index,
                };
                let schema = schema_contexts.as_ref().and_then(|contexts| contexts.get(parameter_index)).cloned().flatten();
                let actual = self.check_expr_with_schema_arena(arena, source, crate::syntax::arena::ArenaExprOrRun::Expr(value), None, schema);
                checked.insert(value, actual);
            }
            let expanded = match crate::sema::arguments::expand_named_arguments(arena, args, |expression| checked.get(&expression).cloned()) {
                Ok(expanded) => expanded,
                Err(error) => { self.graph_boundary_error(error.span, &error.message, "check.named-spread"); return Ok(Type::Invalid); }
            };
            let (family, binding, input_roles, path_slots) = {
                let mut state = self.generic.borrow_mut();
                let super::generic::GenericState { facts, registry, .. } = &mut *state;
                let family = if receiver_kind == Some(MethodReceiver::PathConstructor) {
                    registry.type_constructor_family(&mut facts.graph, "Path", name, span)?
                } else { registry.method_family(&mut facts.graph, name, span)? };
                let candidates = facts.graph.family(family)?.to_vec();
                let mut allowed = Vec::new();
                let mut binding = None;
                let mut input_roles = None;
                let mut path_slots: Option<Vec<bool>> = None;
                let mut pure_rejected = false;
                let mut last_binding_error = None;
                for candidate in candidates {
                    let metadata = registry.metadata(&facts.graph, candidate)?;
                    if let Some(receiver_kind) = receiver_kind {
                        let correct = match metadata.owner {
                            crate::sema::registry_graph::RegistryOwner::Method(receiver) => receiver == receiver_kind,
                            crate::sema::registry_graph::RegistryOwner::TypeConstructor(_) => receiver_kind == MethodReceiver::PathConstructor,
                            _ => false,
                        };
                        if !correct { continue; }
                    }
                    let params: Vec<_> = metadata.parameters.iter().map(|parameter| crate::sema::types::CallableParamType {
                        name: parameter.label, ty: Type::Invalid, defaulted: parameter.defaulted, rest: false,
                    }).collect();
                    let candidate_binding = match crate::sema::arguments::bind_static_arguments(&params, &expanded) {
                        Ok(binding) => binding, Err(error) => { last_binding_error = Some((error, params.iter().filter(|parameter| !parameter.defaulted).count(), params.len())); continue; }
                    };
                    if self.in_pure && metadata.kind != CallableKind::Pure { pure_rejected = true; continue; }
                    if binding.as_ref().is_some_and(|prior: &crate::sema::arguments::StaticArgumentBinding| prior.argument_slots != candidate_binding.argument_slots || prior.omitted_slots != candidate_binding.omitted_slots) {
                        return Err(InferenceError::Boundary("method overloads require distinct source argument binding plans"));
                    }
                    let roles: Vec<_> = facts.graph.candidate(candidate)?.effect_roles.iter().map(|(role, _)| *role).collect();
                    if input_roles.as_ref().is_some_and(|prior| prior != &roles) { return Err(InferenceError::InvalidScheme); }
                    input_roles = Some(roles); binding = Some(candidate_binding); allowed.push(candidate);
                    let template = facts.graph.candidate(candidate)?;
                    let crate::sema::inference::TypeNode::Arrow(arrow) = facts.graph.node(facts.graph.scheme(template.scheme)?.body)? else { return Err(InferenceError::InvalidScheme); };
                    let paths: Vec<_> = arrow.params.iter().skip(usize::from(template.has_receiver)).map(|parameter| matches!(facts.graph.node(parameter.ty), Ok(crate::sema::inference::TypeNode::Atom(crate::sema::inference::Atom::Path)))).collect();
                    if let Some(previous) = &mut path_slots {
                        if previous.len() != paths.len() { return Err(InferenceError::InvalidScheme); }
                        for (previous, current) in previous.iter_mut().zip(paths) { *previous &= current; }
                    } else { path_slots = Some(paths); }
                }
                if allowed.is_empty() {
                    drop(state);
                    if pure_rejected { self.graph_boundary_error(span, "effectful method is not allowed in pure functions", "check.pure-effect"); }
                    else if let Some((error, required, maximum)) = last_binding_error {
                        let code = if expanded.len() < required || expanded.len() > maximum { "check.arity" } else { "check.named-arg" };
                        self.graph_boundary_error(if args.is_empty() { span } else { error.span }, &error.message, code);
                    }
                    else { self.graph_boundary_error(span, "no registered method matches receiver", "check.unknown-method"); }
                    return Ok(Type::Invalid);
                }
                (facts.graph.register_family(&allowed)?, binding.unwrap(), input_roles.unwrap(), path_slots.unwrap())
            };
            let mut effect_bindings = Vec::new();
            if !input_roles.is_empty() {
                let base = receiver_expression.ok_or(InferenceError::Boundary("method producer requires its receiver expression"))?;
                let roots = self.producer_effects_for_expression(arena, base, receiver_path, span).ok_or(InferenceError::InvalidScheme)?;
                for role in input_roles {
                    let effect = match role { EffectRole::Pull { source: 0 } => roots.pull, EffectRole::Close { source: 0 } => roots.close,
                        _ => return Err(InferenceError::InvalidScheme) };
                    effect_bindings.push((role, effect));
                }
            }
            let mut supplied = vec![None; binding.argument_slots.len() + binding.omitted_slots.len()];
            let mut actual_arguments = Vec::new();
            let mut argument_coercions = Vec::new();
            for (index, (argument, &slot)) in expanded.iter().zip(&binding.argument_slots).enumerate() {
                let actual = self.graph_type(&argument.ty, argument.span)?;
                let literal_path = path_slots[slot] && matches!(argument.value, crate::sema::arguments::ArgumentValueSource::Expression(expression)
                    if matches!(arena.arena.expr(expression).kind, ArenaExprKind::Str(_)));
                let operand = if literal_path {
                    argument_coercions.push((index, super::solved::RegistryArgumentCoercion::PathLikeToPath));
                    self.graph_type(&Type::Path, argument.span)?
                } else { actual };
                supplied[slot] = Some(operand); actual_arguments.push(actual);
            }
            let expected = expected.filter(|ty| !matches!(ty, Type::Unit | Type::Unknown | Type::Invalid)).map(|ty| self.graph_type(ty, span)).transpose()?;
            let mut state = self.generic.borrow_mut();
            let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
            let output_effect_bindings = {
                let super::generic::GenericState { facts, registry, .. } = &mut *state;
                registry.output_effect_bindings(&mut facts.graph, family, level)?
            };
            let graph = &mut state.facts.graph;
            let result = graph.fresh(level, span)?;
            let effects = EffectSummary::Variable(if effect_bindings.is_empty() { graph.fresh_execution_effect(None)? } else { graph.fresh_derived_effect_at(level, None)? });
            let reason = graph.reason(span, None)?;
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver, arguments: supplied, result, effects, effect_bindings, output_effect_bindings: output_effect_bindings.clone() }, reason)?;
            graph.solve()?;
            if let Some(expected) = expected {
                // A success expectation constrains the payload without changing
                // the native call's declared Result carrier.
                let contextual_result = match (graph.node(graph.resolved(expected)?)?, graph.node(graph.resolved(result)?)?) {
                    (crate::sema::inference::TypeNode::Result(_, _) | crate::sema::inference::TypeNode::Meta(_) | crate::sema::inference::TypeNode::Rigid { .. }, _) => result,
                    (_, crate::sema::inference::TypeNode::Result(value, _)) => *value,
                    _ => result,
                };
                if let (Ok(expected_view), Ok(actual_view)) = (graph.export_type(expected), graph.export_type(contextual_result)) && actual_view.any_flows_to_concrete(&expected_view) {
                    drop(state);
                    self.graph_boundary_error(span, &format!("unchecked {actual_view} cannot establish {expected_view}; validate with `.require(Type)` or use a checked type pattern"), "check.dynamic-boundary");
                    return Ok(Type::Invalid);
                }
                graph.assignable(expected, contextual_result, reason)?; graph.solve()?;
            }
            let required_effect = if let Some(evidence) = graph.candidate_evidence(requirement)? {
                let selected = evidence.candidate;
                state.registry.metadata(&state.facts.graph, selected)?.required_effect.clone()
            } else { None };
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.push(requirement); }
            state.facts.operations.insert(identity, SolvedOperation { argument_coercions, requirement, result, effects, receiver, actual_arguments,
                binding: CallBinding { dynamic: None, supplied_slots: binding.argument_slots.clone(), default_slots: binding.omitted_slots.clone(), rest_slot: None }, caller: self.current_generic });
            state.facts.expressions.insert(identity, result);
            if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(identity, owner); }
            drop(state);
            self.record_registry_operation_producer_flow_with_receiver_path(arena, expression, requirement, receiver.and(receiver_expression), receiver_path, &expanded, &binding, span)?;
            if let Some(effect) = required_effect { self.require_effect(effect, span, &format!("method `{name}`")); }
            self.record_graph_effect_summary(effects, span);
            Ok(self.graph_view(result))
        })();
        match outcome { Ok(ty) => ty, Err(error) => { self.graph_error(span, error); Type::Invalid } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn source_every_registered_method_retains_its_canonical_operation_authority() {
        use crate::sema::inference::{Atom, InferenceContext, TypeNode};
        use crate::sema::registry_graph::{RegistryGraph, RegistryOwner};
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut registry = RegistryGraph::default();
        let span = Span::new(SourceId::new(0), 0, 1);
        let mut candidates = std::collections::BTreeSet::new();
        for (receiver, methods) in super::super::api_spec().method_entries() {
            for method in methods {
                let family = if receiver == MethodReceiver::PathConstructor {
                    registry.type_constructor_family(&mut graph, "Path", method.name, span).unwrap()
                } else { registry.method_family(&mut graph, method.name, span).unwrap() };
                candidates.extend(graph.family(family).unwrap().iter().copied());
            }
        }
        assert_eq!(candidates.len(), 157);
        let reason = graph.reason(span, None).unwrap(); let string = graph.atom(Atom::Str).unwrap();
        for (index, candidate) in candidates.into_iter().enumerate() {
            let metadata = registry.metadata(&graph, candidate).unwrap().clone();
            let instance = graph.instantiate(metadata.scheme, 1, reason).unwrap();
            for substitution in &instance.substitutions { graph.unify(*substitution, string, reason).unwrap(); }
            graph.solve().unwrap();
            let TypeNode::Arrow(arrow) = graph.node(instance.ty).unwrap() else { panic!("registry method is not callable") };
            let mut parameters = Vec::new(); let mut arguments = Vec::new();
            let constructor = matches!(metadata.owner, RegistryOwner::TypeConstructor(_));
            for (slot, parameter) in arrow.params.iter().enumerate() {
                let name = if !constructor && slot == 0 { "receiver".to_string() } else { format!("argument_{slot}") };
                let ty = graph.export_type(parameter.ty).unwrap();
                parameters.push(format!("{name}: {ty}"));
                if constructor || slot != 0 { arguments.push(format!("{}: {name}", parameter.label)); }
            }
            let target = if constructor { "Path" } else { "receiver" };
            let source = format!("proc method_{index}({}) [fs,net,process,time,env,io,error] {{ {target}.{}({}) }}\n", parameters.join(", "), metadata.entry, arguments.join(", "));
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{}: {source} {:?}", metadata.public_label, parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{}: {source} {:?}", metadata.public_label, checked.diagnostics);
            assert_eq!(checked.solved.operations.len(), 1, "{}", metadata.public_label);
            let operation = checked.solved.operations.values().next().unwrap();
            let evidence = checked.solved.graph.candidate_evidence(operation.requirement).unwrap().unwrap();
            let authority = checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap();
            assert!(matches!(authority, super::super::operation_catalog::SolvedOperationAuthority::Registry(actual) if actual.operation == metadata.operation));
            checked.solved.validate().unwrap();
            let extra = if arguments.is_empty() { "unexpected: 1".to_string() } else { format!("{}, unexpected: 1", arguments.join(", ")) };
            let invalid = format!("proc method_{index}({}) [fs,net,process,time,env,io,error] {{ {target}.{}({extra}) }}\n", parameters.join(", "), metadata.entry);
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &invalid);
            assert!(parsed.diagnostics.is_empty(), "{invalid} {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &invalid);
            assert!(checked.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code.as_deref(), Some("check.arity" | "check.named-arg"))), "{} accepted an extra argument: {:?}", metadata.public_label, checked.diagnostics);
        }
    }

    #[test]
    fn source_standard_method_retains_receiver_item_result_through_forwarding() {
        let declarations = "pure appended(values, item) { values.push(item: item) }\npure forwarded(values, item) { appended(item: item, values: values) }\n";
        for callers in [
            "let numbers: List[Int] = forwarded([1], 2)\nlet words: List[Str] = forwarded([\"one\"], \"two\")\n",
            "let words: List[Str] = forwarded([\"one\"], \"two\")\nlet numbers: List[Int] = forwarded([1], 2)\n",
        ] {
            let source = format!("{declarations}{callers}");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert_eq!(checked.solved.operations.len(), 1);
            let operation = checked.solved.operations.values().next().unwrap();
            assert!(operation.receiver.is_some());
            assert_eq!(operation.binding.supplied_slots, [0]);
            for declaration in checked.solved.declarations.values() {
                assert_eq!(checked.solved.graph.scheme(declaration.scheme).unwrap().requirements.len(), 1);
            }
            checked.solved.validate().unwrap();
        }
        let source = format!("{declarations}let invalid: List[Int] = forwarded([1], \"two\")\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))), "{:?}", checked.diagnostics);
    }

    #[test]
    fn source_method_argument_retains_the_receiver_schema_application() {
        let source = "type Marker[T] = {name: Str}\nlet values: List[Marker[Int]] = []\nlet retained = values.push(Marker(name: \"kept\"))\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.operations.len(), 1);
        assert_eq!(checked.solved.constructor_applications.len(), 1);
        drop(parsed);
        checked.solved.validate().unwrap();
        let source = "type Marker[T] = {name: Str}\nlet values = [{name: \"plain\"}]\nlet guessed = values.push(Marker(name: \"missing\"))\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.constructor-inference")), "{:?}", checked.diagnostics);
    }

    #[test]
    fn source_result_context_rejects_arguments_outside_the_registered_contract() {
        let source = "let original: Result[Int] = Ok(7)\nlet changed = original.context(\"kind\", \"message\", 1)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.arity")), "{:?}", checked.diagnostics);
    }

    #[test]
    fn source_path_literal_coercion_retains_the_original_argument_type() {
        let source = "pure relative(value: Path) -> Result[Path] { value.strip_prefix(prefix: \"base\") }\nlet value = relative(Path(\"base/item\"))\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let operation = checked.solved.operations.values().next().expect("registered method owns a graph operation");
        assert_eq!(operation.argument_coercions, [(0, super::super::solved::RegistryArgumentCoercion::PathLikeToPath)]);
        assert_eq!(checked.solved.graph.export_type(operation.actual_arguments[0]).unwrap(), Type::Str);
        assert_eq!(operation.binding.supplied_slots, [0]);
        checked.solved.validate().unwrap();

        let source = "pure relative(value: Path, prefix: Str) -> Result[Path] { value.strip_prefix(prefix: prefix) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", checked.diagnostics);
    }

    #[test]
    fn source_native_method_keeps_creation_and_producer_permissions_separate() {
        let source = "proc opened(file: Path) [fs] -> Result[Stream[Str]] { file.lines() }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let operation = checked.solved.operations.values().next().unwrap();
        let evidence = checked.solved.graph.candidate_evidence(operation.requirement).unwrap().unwrap();
        assert_eq!(evidence.effects, EffectSummary::Closed(crate::sema::inference::EffectSet::FS));
        assert_eq!(evidence.effect_roots, [EffectSummary::Closed(crate::sema::inference::EffectSet::FS), EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY)]);
        checked.solved.validate().unwrap();
        for (source, code) in [
            ("proc opened(file: Path) [] -> Result[Stream[Str]] { file.lines() }\n", "check.effect-violation"),
            ("pure opened(file: Path) -> Result[Stream[Str]] { file.lines() }\n", "check.pure-effect"),
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some(code)), "{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn source_materialized_line_collectors_keep_independent_receiver_candidates() {
        let source = "pure collected(value) { value.lines().collect() }\npure forwarded(value) { collected(value) }\nlet words: List[Str] = forwarded(\"one\\ntwo\")\nlet raw: List[Bytes] = forwarded(b\"one\\ntwo\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.operations.len(), 2);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn source_record_get_retains_the_checked_field_value_flow() {
        let source = "let record = {rows: [\"one\"], count: 7}\nlet selected = record.get(\"rows\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let _symbols = checked.solved.symbol_owner().enter();
        assert!(checked.solved.producer_flows.nodes().any(|node| matches!(&node.kind,
            super::super::ProducerFlowKind::Project { path, .. }
                if path.0 == [super::super::ProducerPathComponent::RecordField(crate::symbol::Name::intern("rows"))]
        )), "checked record field must retain its own value flow");
        checked.solved.validate().unwrap();
    }

    #[test]
    fn source_propagated_record_field_collect_keeps_the_selected_producer_budget() {
        let prefix = "stream clocked() [time] { let value = time.now(); yield 1 }\n";
        for (permissions, accepted) in [("time, error", true), ("error", false)] {
            let source = format!("{prefix}proc consumed() [{permissions}] -> Result[List[Int]] {{ let record = {{rows: clocked(), ignored: [\"local\"]}}; record.get(\"rows\")?.collect() }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            if accepted { assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics); checked.solved.validate().unwrap(); }
            else { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))), "propagated producer must retain TIME: {:?}", checked.diagnostics); }
        }
    }
}
