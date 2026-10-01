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
    let [NormalizedCallableAuthority::Native(contract)] = alternatives.as_slice() else { panic!("single reference has exactly its canonical native authority") };
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
    let direct = query.type_view(&Type::Graph(reference.signature), None).unwrap();
    assert_eq!(direct.shape().callable_signature(), Some(arrow));
    assert_eq!(result.semantic_parity(&direct), Ok(false));
    let mut changed = result.clone();
    let NormalizedShape::NativeCallable { alternatives, .. } = &mut changed.shape else { unreachable!() };
    let NormalizedCallableAuthority::Native(contract) = &mut alternatives[0] else { unreachable!() };
    std::sync::Arc::make_mut(contract).actual_eligibility.clear();
    assert_eq!(result.semantic_parity(&changed), Ok(false));
    let flow = query.expression_producer_flow(identity).unwrap();
    assert!(flow.nodes.iter().any(|node| matches!(&node.kind, NormalizedProducerFlowKind::NativeCallable { contract: flow_contract } if flow_contract.as_ref() == native_contract(result.shape()))));
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
        assert_eq!(native.contract.candidate.public_label, "json.encode");
        assert!(checked.solved.registry_references.keys().any(|source| source.source == native.origin.source && source.expression == native.origin.expression));
        let NormalizedRequirement::Operation { mono_contract: Some(contract), arguments, .. } = &native.operation else { panic!("native invocation retains its child operation receipt") };
        assert!(std::sync::Arc::ptr_eq(contract, &native.contract), "child proof shares the normalized monotype authority");
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
    let NormalizedCallableAuthority::Native(contract) = &mut alternatives[0] else { unreachable!() };
    std::sync::Arc::make_mut(contract).candidate.output_effect_roles[0].1 = u32::MAX;
    assert_eq!(escaped.semantic_parity(&escaped), Err(ParityError::UnscopedBinder));
    let path = NormalizedProducerPath(vec![NormalizedProducerPathComponent::ResultSuccess]);
    let expected = NormalizedProducerEffects { pull: NormalizedEffect::Closed(vec!["process".to_string()]), close: NormalizedEffect::Closed(vec![]) };
    let produced = checked.solved.expression_producers.iter().find(|(_, profile)| profile.iter().any(|(path, effects)| path.0 == [crate::sema::check::ProducerPathComponent::ResultSuccess] && effects.pull == EffectSummary::Closed(EffectSet::PROCESS))).map(|(&identity, _)| identity).expect("known returned native factory profile");
    assert_eq!(query.expression_producers(produced).unwrap().get(&path), Some(&expected));
    let flow = query.expression_producer_flow(produced).unwrap();
    assert!(flow.nodes.iter().any(|node| matches!(&node.kind, NormalizedProducerFlowKind::NativeCallable { contract } if contract.candidate.public_label == "process.list")));
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
        let NormalizedCallableAuthority::Native(contract) = &alternatives[1] else { panic!("native alternative remains separate") };
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
