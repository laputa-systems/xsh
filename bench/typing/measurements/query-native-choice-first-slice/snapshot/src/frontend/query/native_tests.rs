use super::*;
use crate::sema::check::{Checker, CheckOutput};
use crate::source::SourceId;
use crate::syntax::parser::Parser;

fn checked_source(source: &str) -> CheckOutput {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    drop(parsed);
    assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
    checked.solved.validate().unwrap();
    checked
}

fn native_contract(shape: &NormalizedShape) -> &NormalizedNativeContract {
    let NormalizedShape::NativeCallable { alternatives, .. } = shape else { panic!("native value must retain its authority independently of the mono signature") };
    let [NormalizedCallableAuthority::Native(NormalizedNativeAuthority::Single(contract))] = alternatives.as_slice() else { panic!("single reference has exactly its canonical native authority") };
    contract
}

#[test]
fn solved_query_native_json_reference_retains_mono_signature_guards_after_ast_drop() {
    let checked = checked_source("let encode = json.encode\n");
    let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
    let before = checked.solved.graph.counters().instantiations;
    let (&identity, reference) = checked.solved.registry_references.iter().next().unwrap();
    let result = query.expression(identity).unwrap();
    let arrow = result.shape().callable_signature().unwrap();
    assert_eq!(arrow.kind, CallableForm::Pure);
    assert_eq!(arrow.parameters.iter().map(|parameter| (parameter.label.as_str(), parameter.defaulted, parameter.rest)).collect::<Vec<_>>(), [("value", false, false), ("pretty", true, false)]);
    let contract = native_contract(result.shape());
    assert_eq!(contract.candidate.public_label, "json.encode");
    assert_eq!(contract.actual_eligibility, [(0, EligibilityPredicate::JsonCompatible)]);
    assert_eq!(contract.argument_relations, [NormalizedArgumentRelation::DeclaredErasure, NormalizedArgumentRelation::Assignable]);
    assert!(!contract.has_receiver);
    assert_eq!(contract.signature.callable_signature(), Some(arrow));
    assert_eq!(contract.owner, checked.solved.owner);
    assert_eq!(result.annotation_source(), None);
    let callable = query.expression_callable(identity).unwrap();
    assert_eq!(callable.scheme.ty.shape(), result.shape());
    assert_eq!(result.semantic_parity(&result), Ok(true));
    assert!(result.to_string().contains("json.encode"));
    let direct = query.type_view(&Type::Graph(checked.solved.graph.callable_signature(reference.value_type()).unwrap()), None).unwrap();
    assert_eq!(direct.shape().callable_signature(), Some(arrow));
    assert_eq!(result.semantic_parity(&direct), Ok(false));
    let mut changed = result.clone();
    let NormalizedShape::NativeCallable { alternatives, .. } = &mut changed.shape else { unreachable!() };
    let NormalizedCallableAuthority::Native(NormalizedNativeAuthority::Single(contract)) = &mut alternatives[0] else { unreachable!() };
    std::sync::Arc::make_mut(contract).actual_eligibility.clear();
    assert_eq!(result.semantic_parity(&changed), Ok(false));
    let flow = query.expression_producer_flow(identity).unwrap();
    assert!(flow.nodes.iter().any(|node| matches!(&node.kind, NormalizedProducerFlowKind::NativeCallable { authority: NormalizedNativeAuthority::Single(flow_contract) } if flow_contract.as_ref() == native_contract(result.shape()))));
    assert_eq!(flow.semantic_parity(&flow), Ok(true));
    assert_eq!(SolvedQuery::new(&checked.solved, checked.solved.symbol_owner()).with_limits(QueryLimits { depth: 1, ..QueryLimits::default() }).expression(identity), Err(QueryError::Limit));
    assert_eq!(checked.solved.graph.counters().instantiations, before);
}

#[test]
fn solved_query_actual_callback_type_keeps_principal_scheme_separate_after_ast_drop() {
    let checked = checked_source("pure identity(value) { value }\npure apply(callback, value) { callback(value) }\nlet result: Int = apply(identity, 7)\n");
    let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
    let before = checked.solved.graph.counters().instantiations;
    let identity = checked.solved.expression_callables.keys().find(|&&identity| {
        checked.solved.expression_callables[&identity].declaration.is_some_and(|declaration| checked.solved.graph.callable_signature(checked.solved.declarations[&declaration].signature).ok().and_then(|signature| checked.solved.graph.node(signature).ok()).is_some_and(|node| matches!(node, TypeNode::Arrow(arrow) if arrow.params.len() == 1)))
        && checked.solved.expression_owners.get(&identity).is_none()
        && checked.solved.expression_schemes.get(&identity).is_none()
    }).copied().expect("actual top-level callback reference");
    let actual = query.expression(identity).unwrap();
    let principal = query.expression_callable(identity).unwrap();
    assert_eq!(actual.shape().callable_signature().unwrap().parameters[0].ty, NormalizedShape::Atom("Int".to_string()));
    assert_eq!(principal.scheme.quantifiers.len(), 1);
    assert_eq!(principal.scheme.ty.shape().callable_signature().unwrap().parameters[0].ty, NormalizedShape::Binder { index: 0, kind: BinderKind::Type });
    assert_eq!(query.reveal(identity).unwrap(), principal.to_string());
    assert_eq!(checked.solved.graph.counters().instantiations, before);
}

#[test]
fn solved_query_native_invocation_keeps_original_masks_timing_and_shared_authority() {
    let checked = checked_source("let encode = json.encode\nlet first: Result[Str] = encode(1)\nlet second: Result[Str] = encode(pretty: true, value: \"word\")\n");
    let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
    let before = checked.solved.graph.counters().instantiations;
    let mut bindings = Vec::new();
    for &identity in checked.solved.invocations.keys() {
        let result = query.invocation(identity).unwrap();
        assert_eq!(result.semantic_parity(&result), Ok(true));
        let evidence = result.evidence.as_ref().unwrap();
        assert_eq!(evidence.default_timing, NormalizedInvocationDefaultTiming::AtCall);
        assert_eq!(evidence.signature.callable_signature().unwrap().parameters[0].ty, NormalizedShape::Atom("Any".to_string()));
        assert_eq!(evidence.native_alternatives.len(), 1);
        let native = &evidence.native_alternatives[0];
        assert_eq!(match &native.authority { NormalizedNativeAuthority::Single(contract) => &contract.candidate.public_label, _ => panic!("singleton JSON authority") }, "json.encode");
        assert!(checked.solved.registry_references.keys().any(|source| source.source == native.origin.source && source.expression == native.origin.expression));
        let NormalizedRequirement::Operation { mono_authority: Some(NormalizedNativeAuthority::Single(contract)), arguments, .. } = &native.operation else { panic!("native invocation retains its child operation receipt") };
        assert!(match &native.authority { NormalizedNativeAuthority::Single(native_contract) => std::sync::Arc::ptr_eq(contract, native_contract), _ => false }, "child proof shares the normalized monotype authority");
        assert!(std::sync::Arc::ptr_eq(native.selected_member.as_ref().expect("selected canonical child receipt"), contract));
        assert!(!matches!(arguments[0].as_ref().unwrap(), NormalizedShape::Atom(name) if name == "Any"), "original actual is preserved before declared erasure");
        bindings.push((evidence.binding.supplied_slots.clone(), evidence.binding.default_slots.clone()));
        let mut changed = result.clone();
        changed.evidence.as_mut().unwrap().default_timing = NormalizedInvocationDefaultTiming::AtPull;
        assert_eq!(result.semantic_parity(&changed), Ok(false));
        let limited = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner()).with_limits(QueryLimits { nodes: 4, ..QueryLimits::default() });
        assert_eq!(limited.invocation(identity), Err(QueryError::Limit));
    }
    bindings.sort();
    assert_eq!(bindings, [(vec![0], vec![1]), (vec![1, 0], vec![])]);
    let noncall = *checked.solved.registry_references.keys().next().unwrap();
    assert_eq!(query.invocation(noncall), Err(QueryError::MissingCall));
    assert_eq!(query.invocation(ExpressionIdentity { source: SourceId::new(999), ..noncall }), Err(QueryError::MissingExpression));
    assert_eq!(checked.solved.graph.counters().instantiations, before);
}

#[test]
fn solved_query_native_process_factory_keeps_projected_roles_through_returned_value() {
    let source = "pure retain(value) { value }\nproc opened(factory) [process] { factory() }\nlet services = {rows: retain(process.list)}\nlet produced = opened(services.rows)\n";
    let checked = checked_source(source);
    let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
    let before = checked.solved.graph.counters().instantiations;
    let reference = *checked.solved.registry_references.keys().next().unwrap();
    let result = query.expression(reference).unwrap();
    let contract = native_contract(result.shape());
    assert_eq!(contract.candidate.public_label, "process.list");
    assert_eq!(contract.candidate.output_effect_roles, [(NormalizedProducerRole::Pull, 0), (NormalizedProducerRole::Close, 1)]);
    assert_eq!(contract.effect_roots, [NormalizedEffect::Closed(vec!["process".to_string()]), NormalizedEffect::Closed(vec![])]);
    assert_eq!(contract.candidate.output_effect_roots, contract.effect_roots);
    let mut escaped = result.clone();
    let NormalizedShape::NativeCallable { alternatives, .. } = &mut escaped.shape else { unreachable!() };
    let NormalizedCallableAuthority::Native(NormalizedNativeAuthority::Single(contract)) = &mut alternatives[0] else { unreachable!() };
    std::sync::Arc::make_mut(contract).candidate.output_effect_roles[0].1 = u32::MAX;
    assert_eq!(escaped.semantic_parity(&escaped), Err(ParityError::UnscopedBinder));
    let path = NormalizedProducerPath(vec![NormalizedProducerPathComponent::ResultSuccess]);
    let expected = NormalizedProducerEffects { pull: NormalizedEffect::Closed(vec!["process".to_string()]), close: NormalizedEffect::Closed(vec![]) };
    let produced = checked.solved.expression_producers.iter().find(|(_, profile)| profile.iter().any(|(path, effects)| path.0 == [crate::sema::check::ProducerPathComponent::ResultSuccess] && effects.pull == EffectSummary::Closed(EffectSet::PROCESS))).map(|(&identity, _)| identity).expect("known returned native factory profile");
    assert_eq!(query.expression_producers(produced).unwrap().get(&path), Some(&expected));
    let flow = query.expression_producer_flow(produced).unwrap();
    assert!(flow.nodes.iter().any(|node| matches!(&node.kind, NormalizedProducerFlowKind::NativeCallable { authority: NormalizedNativeAuthority::Single(contract) } if contract.candidate.public_label == "process.list")));
    assert_eq!(flow.semantic_parity(&flow), Ok(true));
    assert_eq!(checked.solved.graph.counters().instantiations, before);
}

#[test]
fn solved_query_native_user_selection_keeps_both_authorities_in_either_source_order() {
    let mut answers = Vec::new();
    for (first, second) in [("json.encode", "fallback"), ("fallback", "json.encode")] {
        let source = format!("pure fallback(value: Any, pretty: Bool = false) -> Result[Str] {{ Ok(\"fallback\") }}\npure choose(select: Bool) {{ if select {{ ({first}) }} else {{ ({second}) }} }}\nlet encode = choose(true)\n");
        let checked = checked_source(&source);
        let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        let before = checked.solved.graph.counters().instantiations;
        let (identity, _) = checked.solved.calls.iter().find(|(_, call)| checked.solved.graph.resolved(call.signature).ok().and_then(|signature| checked.solved.graph.node(signature).ok()).is_some_and(|node| matches!(node, TypeNode::Arrow(arrow) if checked.solved.graph.resolved(arrow.result).ok().and_then(|result| checked.solved.graph.node(result).ok()).is_some_and(|node| matches!(node, TypeNode::NativeCallable(_)))))).unwrap();
        let result = query.expression(*identity).unwrap();
        let NormalizedShape::NativeCallable { signature, alternatives } = result.shape() else { panic!("selection preserves native and authored callable authorities") };
        assert_eq!(alternatives.len(), 2);
        assert!(matches!(alternatives[0], NormalizedCallableAuthority::User { .. }));
        let NormalizedCallableAuthority::Native(NormalizedNativeAuthority::Single(contract)) = &alternatives[1] else { panic!("native alternative remains separate") };
        assert_eq!(contract.actual_eligibility, [(0, EligibilityPredicate::JsonCompatible)]);
        assert_eq!(result.shape().callable_signature(), signature.callable_signature());
        assert_eq!(result.annotation_source(), None);
        assert_eq!(result.semantic_parity(&result), Ok(true));
        let mut changed = result.clone();
        let NormalizedShape::NativeCallable { alternatives, .. } = &mut changed.shape else { unreachable!() };
        alternatives.remove(1);
        assert_eq!(result.semantic_parity(&changed), Ok(false));
        assert_eq!(checked.solved.graph.counters().instantiations, before);
        answers.push(result);
    }
    assert_eq!(answers[0].to_string(), answers[1].to_string());
    assert_eq!(answers[0].semantic_parity(&answers[1]), Err(ParityError::ForeignNativeOwner));
}

fn native_family(shape: &NormalizedShape) -> &std::sync::Arc<NormalizedNativeFamilyContract> {
    let NormalizedShape::NativeCallable { alternatives, .. } = shape else { panic!("native family retains its mono callable") };
    let [NormalizedCallableAuthority::Native(NormalizedNativeAuthority::Family(family))] = alternatives.as_slice() else { panic!("overload choice is one family authority") };
    family
}

#[test]
fn solved_query_native_command_family_keeps_finite_domains_and_each_member_default_after_ast_drop() {
    let checked = checked_source("let command = process.command_argv\n");
    let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
    let before = checked.solved.graph.counters().clone();
    let identity = *checked.solved.registry_references.keys().next().unwrap();
    let result = query.expression(identity).unwrap();
    let family = native_family(result.shape());
    assert_eq!(family.owner, checked.solved.owner);
    assert_eq!(family.members.len(), 8);
    let arrow = result.shape().callable_signature().unwrap();
    assert_eq!(arrow.parameters.len(), 15);
    assert_eq!(arrow.kind, CallableForm::Pure);
    assert_eq!(arrow.effects, NormalizedEffect::Closed(vec![]));
    assert_eq!(family.signature.callable_signature(), Some(arrow));
    assert_eq!(arrow.parameters.iter().map(|parameter| parameter.label.as_str()).collect::<Vec<_>>(), ["target", "argv", "cwd", "env", "stdin", "stdout", "stderr", "stdout_append", "stderr_append", "timeout", "detach", "new_session", "ignore_hup", "cpu_max", "accept"]);
    assert!(!arrow.parameters[0].defaulted && !arrow.parameters[1].defaulted);
    assert!(arrow.parameters[2..].iter().all(|parameter| parameter.defaulted && !parameter.rest));
    let NormalizedShape::FiniteDomain(targets) = &arrow.parameters[0].ty else { panic!("target alternatives retain canonical admission relations") };
    assert_eq!(targets, &[
        NormalizedDomainAlternative { ty: NormalizedShape::Atom("Path".to_string()), relation: NormalizedArgumentRelation::CommandTarget { domain: NormalizedCommandTextDomain::Path } },
        NormalizedDomainAlternative { ty: NormalizedShape::Atom("Str".to_string()), relation: NormalizedArgumentRelation::CommandTarget { domain: NormalizedCommandTextDomain::Str } },
    ]);
    let NormalizedShape::FiniteDomain(argv) = &arrow.parameters[1].ty else { panic!("argv alternatives remain independently typed") };
    assert_eq!(argv.len(), 2);
    assert_eq!(argv[0].relation, NormalizedArgumentRelation::CommandArgv { element: NormalizedCommandTextDomain::Path });
    assert_eq!(argv[1].relation, NormalizedArgumentRelation::CommandArgv { element: NormalizedCommandTextDomain::Str });
    let mut members = Vec::new();
    for member in &family.members {
        assert_eq!(member.candidate.public_label, "process.command_argv");
        assert_eq!(member.actual_eligibility, [(0, EligibilityPredicate::CommandTarget), (1, EligibilityPredicate::CommandArgv)]);
        let mono = member.signature.callable_signature().unwrap();
        assert_eq!(mono.parameters.len(), 15);
        assert!(member.prototype.quantifiers.is_empty());
        assert_eq!(member.prototype.ty.shape().callable_signature(), Some(mono));
        let stdin = &mono.parameters[4];
        assert_eq!(stdin.defaulted, matches!(&stdin.ty, NormalizedShape::Atom(name) if name == "Path"));
        assert!(matches!(&stdin.ty, NormalizedShape::Atom(name) if name == "Path" || name == "Bytes"));
        members.push((mono.parameters[0].ty.to_string(), mono.parameters[1].ty.to_string(), stdin.ty.to_string(), stdin.defaulted));
    }
    members.sort(); members.dedup();
    assert_eq!(members.len(), 8, "full member tuples preserve canonical cross-slot relationships");
    assert_eq!(result.annotation_source(), None);
    assert_eq!(result.semantic_parity(&result), Ok(true));
    let mut flattened = result.clone();
    let NormalizedShape::NativeCallable { alternatives, .. } = &mut flattened.shape else { unreachable!() };
    *alternatives = family.members.iter().cloned().map(|member| NormalizedCallableAuthority::Native(NormalizedNativeAuthority::Single(member))).collect();
    assert_eq!(result.semantic_parity(&flattened), Ok(false), "one overload choice is distinct from requiring all members as conditional authorities");
    let mut changed = result.clone();
    let NormalizedShape::NativeCallable { alternatives, .. } = &mut changed.shape else { unreachable!() };
    let NormalizedCallableAuthority::Native(NormalizedNativeAuthority::Family(family)) = &mut alternatives[0] else { unreachable!() };
    std::sync::Arc::make_mut(family).members.pop();
    assert_eq!(result.semantic_parity(&changed), Ok(false));
    let mut foreign = result.clone();
    let NormalizedShape::NativeCallable { alternatives, .. } = &mut foreign.shape else { unreachable!() };
    let NormalizedCallableAuthority::Native(NormalizedNativeAuthority::Family(family)) = &mut alternatives[0] else { unreachable!() };
    std::sync::Arc::make_mut(family).owner = crate::sema::inference::InferenceContext::new(crate::sema::inference::Limits::default()).owner();
    assert_eq!(foreign.semantic_parity(&foreign), Err(ParityError::ForeignNativeOwner));
    let flow = query.expression_producer_flow(identity).unwrap();
    assert!(flow.nodes.iter().any(|node| matches!(&node.kind, NormalizedProducerFlowKind::NativeCallable { authority: NormalizedNativeAuthority::Family(retained) } if retained.as_ref() == native_family(result.shape()).as_ref())));
    assert_eq!(flow.semantic_parity(&flow), Ok(true));
    assert_eq!(query.with_limits(QueryLimits { nodes: 8, ..QueryLimits::default() }).expression(identity), Err(QueryError::Limit));
    assert_eq!(checked.solved.graph.counters().instantiations, before.instantiations);
    assert_eq!(checked.solved.graph.counters().attempted_constraints, before.attempted_constraints);
    assert_eq!(checked.solved.graph.counters().unifications, before.unifications);
}

#[test]
fn solved_query_native_command_invocations_keep_selected_member_and_original_default_masks() {
    let checked = checked_source("pure retain(value) { value }\nlet services = {command: retain(process.command_argv)}\nlet first: Command = services.command(\"echo\", [\"word\"])\nlet second: Command = services.command(p\"echo\", [p\"word\"], stdin: b\"input\")\n");
    let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
    let before = checked.solved.graph.counters().clone();
    let mut answers = Vec::new();
    for &identity in checked.solved.invocations.keys() {
        let result = query.invocation(identity).unwrap();
        let evidence = result.evidence.as_ref().unwrap();
        assert_eq!(evidence.default_timing, NormalizedInvocationDefaultTiming::AtCall);
        let [native] = evidence.native_alternatives.as_slice() else { panic!("one overloaded reference is one invocation authority") };
        let NormalizedNativeAuthority::Family(family) = &native.authority else { panic!("overload family is retained independently from selected member") };
        assert_eq!(family.members.len(), 8);
        let selected = native.selected_member.as_ref().expect("child operation supplies the selected canonical member");
        assert!(family.members.iter().any(|member| std::sync::Arc::ptr_eq(member, selected)), "selected receipt shares its retained family member");
        let NormalizedRequirement::Operation { mono_authority: Some(NormalizedNativeAuthority::Family(child_family)), arguments, .. } = &native.operation else { panic!("child operation retains the same family monotype") };
        assert!(std::sync::Arc::ptr_eq(family, child_family));
        assert_eq!(arguments.len(), 15);
        assert_eq!(arguments[0].as_ref(), Some(&selected.signature.callable_signature().unwrap().parameters[0].ty));
        let raw_evidence = checked.solved.graph.invocation_evidence(checked.solved.invocations[&identity].requirement).unwrap().unwrap();
        let [raw_native] = raw_evidence.native_alternatives.as_slice() else { unreachable!() };
        let crate::sema::inference::RequirementTemplate::Operation { call: raw_operation, .. } = checked.solved.graph.requirement_template(raw_native.operation).unwrap() else { unreachable!() };
        let raw_operation = checked.solved.graph.operation_call(raw_operation).unwrap();
        let crate::sema::inference::RequirementTemplate::CallableInvocation { call: raw_call } = checked.solved.graph.requirement_template(checked.solved.invocations[&identity].requirement).unwrap() else { unreachable!() };
        let raw_call = checked.solved.graph.invocation_call(raw_call).unwrap();
        assert_eq!(arguments[1].as_ref(), Some(&selected.signature.callable_signature().unwrap().parameters[1].ty), "original frozen invocation argument is {:?}; child argument is {:?}", checked.solved.graph.export_type(raw_call.arguments[1].ty), raw_operation.arguments[1].map(|ty| checked.solved.graph.export_type(ty)));
        let selected_arrow = selected.signature.callable_signature().unwrap();
        answers.push((selected_arrow.parameters[0].ty.to_string(), selected_arrow.parameters[1].ty.to_string(), selected_arrow.parameters[4].ty.to_string(), selected_arrow.parameters[4].defaulted, evidence.binding.supplied_slots.clone(), evidence.binding.default_slots.clone()));
        assert_eq!(result.semantic_parity(&result), Ok(true));
        let mut changed = result.clone();
        changed.evidence.as_mut().unwrap().native_alternatives[0].selected_member = None;
        assert_eq!(result.semantic_parity(&changed), Ok(false));
        assert_eq!(SolvedQuery::new(&checked.solved, checked.solved.symbol_owner()).with_limits(QueryLimits { depth: 2, ..QueryLimits::default() }).invocation(identity), Err(QueryError::Limit));
    }
    answers.sort();
    assert_eq!(answers, [
        ("Path".to_string(), "List[Path]".to_string(), "Bytes".to_string(), false, vec![0, 1, 4], vec![2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]),
        ("Str".to_string(), "List[Str]".to_string(), "Path".to_string(), true, vec![0, 1], (2..15).collect()),
    ]);
    assert_eq!(checked.solved.graph.counters().instantiations, before.instantiations);
    assert_eq!(checked.solved.graph.counters().attempted_constraints, before.attempted_constraints);
    assert_eq!(checked.solved.graph.counters().unifications, before.unifications);
}

#[test]
fn solved_query_native_choice_preserves_complete_arities_and_selected_invocation_after_ast_drop() {
    let checked = checked_source("let inspect = process.ports\nlet all = inspect()\nlet one = inspect(7)\n");
    let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
    let before = checked.solved.graph.counters().clone();
    let reference = *checked.solved.registry_references.keys().next().unwrap();
    let shape = query.expression(reference).unwrap();
    let family = native_family(shape.shape());
    assert_eq!(family.members.len(), 2);
    assert_eq!(shape.shape().callable_signature(), None, "a choice never presents its first member as the signature");
    let signatures = shape.shape().callable_signatures().unwrap();
    let mut arities = signatures.iter().map(|signature| signature.parameters.len()).collect::<Vec<_>>();
    arities.sort(); assert_eq!(arities, [0, 1]);
    assert!(family.members.iter().all(|member| member.signature.callable_signature().is_some()));
    let callable = query.expression_callable(reference).unwrap();
    assert_eq!(callable.effective_effects, None, "an unselected choice has no single effective effect summary");
    assert_eq!(callable.scheme.ty.shape(), shape.shape());
    assert_eq!(callable.scheme.ty.annotation_source(), None);
    assert!(query.reveal(reference).unwrap().contains("choice {"));
    assert_eq!(callable.semantic_parity(&callable), Ok(true));
    let mut answers = Vec::new();
    for &identity in checked.solved.invocations.keys() {
        let result = query.invocation(identity).unwrap();
        let evidence = result.evidence.as_ref().expect("actual unique member supplies evidence");
        let signature = evidence.signature.callable_signature().unwrap();
        let [native] = evidence.native_alternatives.as_slice() else { panic!("one family supplies one child operation") };
        let selected = native.selected_member.as_ref().unwrap();
        assert_eq!(selected.signature.callable_signature(), Some(signature));
        assert_eq!(native.selected_binding.as_ref(), Some(&evidence.binding));
        let NormalizedRequirement::Operation { binding: NormalizedOperationBinding::Invocation(original), effect_mode, arguments, .. } = &native.operation else { panic!("member selection retains original invocation binding instead of invented formal slots") };
        assert_eq!(*effect_mode, NormalizedOperationEffectMode::ComputedCreation);
        assert!(arguments.is_empty());
        assert_eq!(original.domain, NormalizedCallableDomain::AnyCallable);
        assert!(matches!(original.callable, NormalizedShape::NativeCallable { ref signature, .. } if matches!(signature.as_ref(), NormalizedShape::CallableChoice(_))));
        assert_eq!(original.arguments.len(), signature.parameters.len());
        assert!(original.arguments.iter().all(|argument| argument.kind == NormalizedInvocationArgumentKind::Positional));
        assert_eq!(native.selected_actual_arguments.len(), signature.parameters.len());
        assert!(evidence.binding.default_slots.is_empty());
        assert_eq!(evidence.default_timing, NormalizedInvocationDefaultTiming::AtCall);
        answers.push((signature.parameters.len(), evidence.binding.supplied_slots.clone()));
        assert_eq!(result.semantic_parity(&result), Ok(true));
        let mut changed = result.clone();
        let NormalizedRequirement::Operation { effect_mode, .. } = &mut changed.evidence.as_mut().unwrap().native_alternatives[0].operation else { unreachable!() };
        *effect_mode = NormalizedOperationEffectMode::AvailableBudget;
        assert_eq!(result.semantic_parity(&changed), Ok(false));
    }
    answers.sort(); assert_eq!(answers, [(0, vec![]), (1, vec![0])]);
    assert_eq!(checked.solved.graph.counters().instantiations, before.instantiations);
    assert_eq!(checked.solved.graph.counters().attempted_constraints, before.attempted_constraints);
    assert_eq!(checked.solved.graph.counters().unifications, before.unifications);
}

#[test]
fn solved_query_native_hash_choice_keeps_pure_and_proc_effects_labels_and_results_distinct() {
    let checked = checked_source("let digest = hash.sha256\nlet bytes_digest: Digest = digest(b\"word\")\nlet path_digest: Result[Digest] = digest(p\"word\")\n");
    let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
    let before = checked.solved.graph.counters().clone();
    let reference = *checked.solved.registry_references.keys().next().unwrap();
    let callable = query.expression_callable(reference).unwrap();
    assert_eq!(callable.effective_effects, None);
    assert_eq!(callable.scheme.ty.shape().callable_signature(), None);
    let signatures = callable.scheme.ty.shape().callable_signatures().unwrap();
    assert_eq!(signatures.len(), 2);
    let pure = signatures.iter().find(|signature| signature.kind == CallableForm::Pure).unwrap();
    let proc = signatures.iter().find(|signature| signature.kind == CallableForm::Proc).unwrap();
    assert_eq!(pure.parameters.len(), 1); assert_eq!(proc.parameters.len(), 1);
    assert_eq!(pure.parameters[0].label, "data"); assert_eq!(proc.parameters[0].label, "path");
    assert_eq!(pure.parameters[0].ty, NormalizedShape::Atom("Bytes".to_string()));
    assert_eq!(proc.parameters[0].ty, NormalizedShape::Atom("Path".to_string()));
    assert_eq!(pure.effects, NormalizedEffect::Closed(vec![]));
    assert_eq!(proc.effects, NormalizedEffect::Closed(vec![]), "canonical hash Path member retains its published empty effect clause and Proc kind");
    assert_eq!(pure.result.as_ref(), &NormalizedShape::Atom("Digest".to_string()));
    assert!(matches!(proc.result.as_ref(), NormalizedShape::Result(success, _) if success.as_ref() == &NormalizedShape::Atom("Digest".to_string())));
    assert_eq!(callable.scheme.ty.annotation_source(), None);
    assert_eq!(callable.semantic_parity(&callable), Ok(true));
    assert!(query.reveal(reference).unwrap().contains("pure(data: Bytes) [] -> Digest"));
    let mut selected = Vec::new();
    for &identity in checked.solved.invocations.keys() {
        let result = query.invocation(identity).unwrap();
        let evidence = result.evidence.as_ref().unwrap();
        let arrow = evidence.signature.callable_signature().unwrap();
        let [native] = evidence.native_alternatives.as_slice() else { panic!("native hash choice retains canonical member proof") };
        assert_eq!(native.selected_member.as_ref().unwrap().signature.callable_signature(), Some(arrow));
        assert_eq!(native.selected_binding.as_ref(), Some(&evidence.binding));
        selected.push((arrow.kind, arrow.parameters[0].label.clone(), evidence.effects.clone()));
        assert_eq!(result.semantic_parity(&result), Ok(true));
    }
    assert!(selected.contains(&(CallableForm::Pure, "data".to_string(), NormalizedEffect::Closed(vec![]))));
    assert!(selected.contains(&(CallableForm::Proc, "path".to_string(), NormalizedEffect::Closed(vec![]))));
    assert_eq!(selected.len(), 2);
    assert_eq!(checked.solved.graph.counters().instantiations, before.instantiations);
    assert_eq!(checked.solved.graph.counters().attempted_constraints, before.attempted_constraints);
    assert_eq!(checked.solved.graph.counters().unifications, before.unifications);
}

#[test]
fn solved_query_callable_choice_preserves_each_members_distinct_effects_and_full_signature() {
    use crate::sema::inference::{Arrow, InferenceContext, Parameter};
    let symbols = SymbolOwner::new();
    let _guard = symbols.enter();
    let mut graph = InferenceContext::default();
    let integer = graph.atom(Atom::Int).unwrap();
    let string = graph.atom(Atom::Str).unwrap();
    let pure = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("item"), ty: integer, defaulted: false, rest: false }], result: integer, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
    let proc = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("text"), ty: string, defaulted: true, rest: false }], result: string, effects: EffectSummary::Closed(EffectSet::TIME) }).unwrap();
    let reason = graph.reason(crate::source::Span::new(SourceId::new(0), 0, 1), None).unwrap();
    let mut candidates = Vec::new();
    for (name, signature) in [("query-choice-pure", pure), ("query-choice-proc", proc)] {
        let scheme = graph.generalize(signature, 0, crate::sema::inference::Generalization::Allowed, &[]).unwrap();
        candidates.push(graph.register_candidate(crate::sema::inference::CandidateTemplate { identity: Name::intern(name), public_label: Name::intern(name), scheme, has_receiver: false, actual_eligibility: Vec::new(), argument_relations: Vec::new(), effect_roles: Vec::new(), output_effect_roles: Vec::new(), failure_projection: None }).unwrap());
    }
    let family = graph.register_family(&candidates).unwrap();
    let instances = candidates.iter().map(|&candidate| { let scheme = graph.candidate(candidate).unwrap().scheme; (candidate, graph.instantiate(scheme, 0, reason).unwrap()) }).collect();
    let native = graph.native_family_callable(family, instances, reason).unwrap();
    let choice = graph.callable_signature(native).unwrap();
    let facts = SolvedTypes::from_graph(graph.freeze(&[native, choice]).unwrap(), symbols.clone());
    let query = SolvedQuery::new(&facts, &symbols);
    let before = facts.graph.counters().clone();
    let result = query.type_view(&Type::Graph(choice), None).unwrap();
    assert_eq!(result.shape().callable_signature(), None);
    let members = result.shape().callable_signatures().unwrap();
    assert_eq!(members.len(), 2);
    let TypeNode::CallableChoice(original) = facts.graph.node(choice).unwrap() else { unreachable!() };
    assert_eq!(members.iter().map(|member| member.kind).collect::<Vec<_>>(), original.iter().map(|&signature| { let TypeNode::Arrow(arrow) = facts.graph.node(signature).unwrap() else { unreachable!() }; callable_form(arrow.kind) }).collect::<Vec<_>>());
    let pure = members.iter().find(|member| member.kind == CallableForm::Pure).unwrap();
    let proc = members.iter().find(|member| member.kind == CallableForm::Proc).unwrap();
    assert_eq!(pure.parameters[0].label, "item");
    assert!(!pure.parameters[0].defaulted);
    assert_eq!(pure.effects, NormalizedEffect::Closed(vec![]));
    assert_eq!(pure.result.as_ref(), &NormalizedShape::Atom("Int".to_string()));
    assert_eq!(proc.parameters[0].label, "text");
    assert!(proc.parameters[0].defaulted);
    assert_eq!(proc.effects, NormalizedEffect::Closed(vec!["time".to_string()]));
    assert_eq!(proc.result.as_ref(), &NormalizedShape::Atom("Str".to_string()));
    assert_eq!(result.annotation_source(), None);
    assert_eq!(result.semantic_parity(&result), Ok(true));
    let mut erased = result.clone();
    let NormalizedShape::CallableChoice(members) = &mut erased.shape else { unreachable!() };
    let NormalizedShape::Arrow(proc) = members.iter_mut().find(|member| matches!(member, NormalizedShape::Arrow(arrow) if arrow.kind == CallableForm::Proc)).unwrap() else { unreachable!() };
    proc.effects = NormalizedEffect::Closed(vec![]);
    assert_eq!(result.semantic_parity(&erased), Ok(false), "a common summary cannot erase a member's required permission");
    assert_eq!(facts.graph.counters().instantiations, before.instantiations);
    assert_eq!(facts.graph.counters().attempted_constraints, before.attempted_constraints);
    assert_eq!(facts.graph.counters().unifications, before.unifications);
}

#[test]
fn solved_query_invocation_protocol_preserves_exact_kinds_without_callable_shape_invention() {
    use crate::sema::inference::{ArgumentRelation, Arrow, CallableDomain, Generalization, InferenceContext, InvocationArgument, InvocationArgumentKind, InvocationCall, Parameter};
    assert_eq!(argument_relation(ArgumentRelation::InvocationProtocol), NormalizedArgumentRelation::InvocationProtocol);
    assert_ne!(argument_relation(ArgumentRelation::InvocationProtocol), NormalizedArgumentRelation::Assignable);
    let mut answers = Vec::new();
    for kind in [CallableKind::Pure, CallableKind::Proc, CallableKind::Stream] {
        let symbols = SymbolOwner::new();
        let _guard = symbols.enter();
        let mut graph = InferenceContext::default();
        let span = crate::source::Span::new(SourceId::new(0), 0, 1);
        let callable = graph.fresh(1, span).unwrap();
        let value = graph.fresh(1, span).unwrap();
        let result = graph.fresh(1, span).unwrap();
        let effects = EffectSummary::Closed(EffectSet::EMPTY);
        let reason = graph.reason(span, None).unwrap();
        let requirement = graph.require_callable_invocation(InvocationCall { callable, arguments: vec![InvocationArgument { kind: InvocationArgumentKind::Positional, ty: value }], result, effects, domain: CallableDomain::Exact(kind) }, reason).unwrap();
        let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("callback"), ty: callable, defaulted: false, rest: false }, Parameter { label: Name::intern("value"), ty: value, defaulted: false, rest: false }], result, effects }).unwrap();
        let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[requirement]).unwrap();
        let solved = SolvedTypes::from_graph(graph.freeze(&[signature]).unwrap(), symbols.clone());
        let before = solved.graph.counters().clone();
        let query = SolvedQuery::new(&solved, &symbols);
        let answer = View::new(&query, Some(scheme)).scheme(signature, Some(scheme)).unwrap();
        let [NormalizedRequirement::CallableInvocation { callable, arguments, domain, .. }] = answer.requirements.as_slice() else { panic!("exact protocol remains a retained obligation") };
        assert_eq!(*domain, NormalizedCallableDomain::Exact(callable_form(kind)));
        assert!(matches!(callable, NormalizedShape::Binder { .. }));
        assert_eq!(arguments[0].kind, NormalizedInvocationArgumentKind::Positional);
        assert!(answer.to_string().contains(&format!("Invoke[exact {}]", match kind { CallableKind::Pure => "pure", CallableKind::Proc => "proc", CallableKind::Stream => "stream" })));
        assert_eq!(answer.semantic_parity(&answer), Ok(true));
        assert_eq!(solved.graph.counters().instantiations, before.instantiations);
        assert_eq!(solved.graph.counters().attempted_constraints, before.attempted_constraints);
        assert_eq!(solved.graph.counters().unifications, before.unifications);
        answers.push(answer);
    }
    assert_eq!(answers[0].semantic_parity(&answers[1]), Ok(false));
    assert_eq!(answers[1].semantic_parity(&answers[2]), Ok(false));
}
