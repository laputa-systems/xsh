use super::{CheckOutput, Checker, ProducerPathComponent};
use crate::sema::inference::{Atom, EffectSet, EffectSummary, InvocationDefaultTiming, RequirementId, RequirementTemplate, TypeNode};
use crate::modules::RuntimeOp;
use crate::source::SourceId;
use crate::syntax::parser::Parser;

// These checks inspect frozen source identities and invocation certificates
// after the parser arena has gone away; they never execute a native factory.
fn checked_source(source: &str) -> CheckOutput {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    drop(parsed);
    checked
}

fn accepted_source(source: &str) -> CheckOutput {
    let checked = checked_source(source);
    assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
    checked.solved.validate().unwrap();
    checked
}

fn rejected_source(source: &str) {
    let checked = checked_source(source);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref()
        .is_some_and(|code| code.starts_with("check."))), "{source}: {:?}", checked.diagnostics);
}

fn native_receipts(checked: &CheckOutput, operation: RuntimeOp) -> Vec<RequirementId> {
    let graph = &checked.solved.graph;
    let mut receipts = std::collections::BTreeSet::new();
    let mut requirements: std::collections::BTreeSet<_> = checked.solved.invocations.values()
        .map(|invocation| invocation.requirement)
        .chain(checked.solved.calls.values().flat_map(|call| call.requirements.iter().copied()))
        .chain(checked.solved.stage_operations.values().filter_map(|stage| match stage.callback {
            Some(super::StageCallback::Callable { requirement, .. }) => Some(requirement),
            Some(super::StageCallback::Protocol { operation, formal_slot, .. }) => graph.candidate_callback_invocation(operation, formal_slot).unwrap(),
            _ => None,
        })).collect();
    let projected: Vec<_> = requirements.iter().filter_map(|&requirement| graph.candidate_evidence(requirement).unwrap())
        .flat_map(|evidence| evidence.callback_invocations.iter().map(|callback| callback.invocation)).collect();
    requirements.extend(projected);
    for requirement in requirements {
        let Some(invocation) = graph.invocation_evidence(requirement).unwrap() else { continue; };
        let RequirementTemplate::CallableInvocation { call: invocation_call } = graph.requirement_template(requirement).unwrap() else { panic!("invocation receipt must belong to its original obligation"); };
        let actual = graph.invocation_call(invocation_call).unwrap();
        for alternative in &invocation.native_alternatives {
            let Some(evidence) = graph.candidate_evidence(alternative.operation).unwrap() else {
                assert!(checked.solved.declarations.values().any(|declaration| declaration.source_requirements.contains(&requirement)),
                    "only a retained generic source obligation may lack selected native evidence");
                continue;
            };
            let contract_id = graph.native_authority_member(alternative.authority, evidence.candidate).unwrap();
            let contract = graph.native_contract(contract_id).unwrap();
            let super::SolvedOperationAuthority::Registry(metadata) = checked.solved.operation_catalog
                .candidate(graph, evidence.candidate).unwrap() else { panic!("native receipt must retain canonical registry authority"); };
            if metadata.operation != operation { continue; }
            let RequirementTemplate::Operation { family, call } = graph.requirement_template(alternative.operation).unwrap() else { panic!("native alternative must retain its canonical operation proof"); };
            let call = graph.operation_call(call).unwrap();
            assert_eq!(family, contract.family);
            assert_eq!(call.mono_authority, Some(alternative.authority));
            assert_eq!(evidence.candidate, contract.candidate);
            assert_eq!(graph.resolved(evidence.signature).unwrap(), graph.resolved(contract.instance.ty).unwrap());
            assert_eq!(evidence.effect_roots, contract.instance.effect_roots);
            assert!(call.receiver.is_none());
            let (signature, invocation_binding, _) = match &invocation.plan {
                crate::sema::inference::InvocationPlan::Unique { signature, binding, timing } => (*signature, binding, *timing),
                crate::sema::inference::InvocationPlan::All { branches } => {
                    let branch = branches.iter().find(|branch| matches!(branch.authority, crate::sema::inference::CallableAuthority::Native { authority } if authority == alternative.authority)).expect("each native alternative retains its original branch plan");
                    (branch.signature, &branch.binding, branch.timing)
                }
            };
            if invocation_binding.dynamic.is_none() {
                let TypeNode::Arrow(signature) = graph.node(graph.resolved(signature).unwrap()).unwrap() else { panic!("each invocation branch keeps its monotype Arrow"); };
                let mut expected = vec![None; signature.params.len()];
                assert_eq!(actual.arguments.len(), invocation_binding.supplied_slots.len());
                for (argument, &slot) in actual.arguments.iter().zip(&invocation_binding.supplied_slots) { expected[slot] = Some(argument.ty); }
                assert_eq!(evidence.actual_arguments, expected, "native guard inputs must preserve original supplied values and default masks");
                match call.binding {
                    crate::sema::inference::OperationBinding::Slots => assert_eq!(call.arguments, expected),
                    crate::sema::inference::OperationBinding::Invocation(parent) => {
                        assert_eq!(parent, invocation_call);
                        assert!(call.arguments.is_empty());
                        let binding = evidence.binding.as_ref().expect("native operation retains the selected member binding");
                        assert_eq!(binding.supplied_slots, invocation_binding.supplied_slots);
                        assert_eq!(binding.default_slots, invocation_binding.default_slots);
                        assert_eq!(call.effect_mode, crate::sema::inference::OperationEffectMode::ComputedCreation);
                    }
                }
            }
            receipts.insert(alternative.operation);
        }
    }
    assert!(!receipts.is_empty(), "successful native source calls require canonical invocation receipts");
    receipts.into_iter().collect()
}

#[test]
fn native_json_callable_retains_guards_through_fields_parameters_and_returns() {
    let source = "pure retain(value) { value }\npure encode_with(callback, value) { callback(value) }\nlet native = json.encode\nlet services = {encode: retain(native)}\nlet first: Result[Str] = encode_with(services.encode, {value: 1})\nlet second: Result[Str] = services.encode(value: [\"two\"], pretty: true)\n";
    let checked = accepted_source(source);
    assert_eq!(checked.solved.registry_references.len(), 1);
    assert!(!checked.solved.invocations.is_empty());
    assert!(checked.solved.expression_callables.values().any(|callable| callable.declaration.is_none()));
    for receipt in native_receipts(&checked, RuntimeOp::JsonEncode) {
        let actual = checked.solved.graph.candidate_evidence(receipt).unwrap().unwrap().actual_arguments[0].unwrap();
        assert!(!matches!(checked.solved.graph.node(checked.solved.graph.resolved(actual).unwrap()).unwrap(), TypeNode::Atom(Atom::Any)),
            "JSON guards must see the original record or list rather than declared Any");
    }
}

#[test]
fn native_json_callable_checks_each_original_actual_before_declared_erasure() {
    let definitions = "pure retain(value) { value }\npure encode_with(callback, value) { callback(value) }\nlet services = {encode: retain(json.encode)}\n";
    accepted_source(&format!("{definitions}let first: Result[Str] = encode_with(services.encode, 1)\nlet second: Result[Str] = encode_with(services.encode, \"word\")\n"));
    for actual in ["b\"raw\"", "Path(\"item\")", "retain", "time.now"] {
        rejected_source(&format!("{definitions}let bad = encode_with(services.encode, {actual})\n"));
    }
}

#[test]
fn native_json_callable_retains_supplied_and_default_parameter_masks() {
    let checked = accepted_source("let encode = json.encode\nlet first: Result[Str] = encode(1)\nlet second: Result[Str] = encode(pretty: true, value: \"word\")\n");
    let bindings: Vec<_> = checked.solved.invocations.values().filter_map(|invocation|
        checked.solved.graph.invocation_evidence(invocation.requirement).unwrap().and_then(|evidence| evidence.unique_plan())).collect();
    assert_eq!(native_receipts(&checked, RuntimeOp::JsonEncode).len(), 2);
    assert!(bindings.iter().any(|(_, binding, timing)| binding.supplied_slots == [0]
        && binding.default_slots == [1] && *timing == InvocationDefaultTiming::AtCall));
    assert!(bindings.iter().any(|(_, binding, timing)| binding.supplied_slots == [1, 0]
        && binding.default_slots.is_empty() && *timing == InvocationDefaultTiming::AtCall));
    for invocation in ["encode()", "encode(1, pretty: 7)", "encode(1, unknown: true)"] {
        rejected_source(&format!("let encode = json.encode\nlet bad = {invocation}\n"));
    }
}

#[test]
fn native_producer_callable_reference_creation_has_no_execution_permissions() {
    let checked = accepted_source("pure factory() { (process.list) }\npure retain(value) { value }\nlet services = {rows: retain(factory())}\n");
    assert_eq!(checked.solved.registry_references.len(), 1);
    assert!(checked.solved.declarations.values().all(|declaration|
        declaration.required_effects == EffectSummary::Closed(EffectSet::EMPTY)));
}

#[test]
fn native_producer_callable_result_keeps_exact_projected_pull_and_cleanup() {
    let source = "pure retain(value) { value }\nproc opened(factory) [process] { factory() }\nlet services = {rows: retain(process.list)}\nlet produced = opened(services.rows)\n";
    let checked = accepted_source(source);
    native_receipts(&checked, RuntimeOp::ProcessList);
    assert!(checked.solved.expression_producers.values().chain(checked.solved.binding_producers.values())
        .any(|profile| profile.iter().any(|(path, effects)|
            path.0 == [ProducerPathComponent::ResultSuccess]
                && effects.pull == EffectSummary::Closed(EffectSet::PROCESS)
                && effects.close == EffectSummary::Closed(EffectSet::EMPTY))),
        "returned native producer must retain its own projected permissions");
}

#[test]
fn native_reference_publication_rejects_removed_or_replaced_authority() {
    for replace_contract in [false, true] {
        let mut checked = accepted_source("let rows = process.list\nlet encode = json.encode\n");
        let facts = std::sync::Arc::get_mut(&mut checked.solved).unwrap();
        let reference_identity = |operation| facts.registry_references.iter().find_map(|(identity, reference)| {
            reference.candidates(&facts.graph).unwrap().iter().any(|&candidate| matches!(facts.operation_catalog.candidate(&facts.graph, candidate).unwrap(),
                super::SolvedOperationAuthority::Registry(metadata) if metadata.operation == operation)).then_some(*identity)
        }).unwrap();
        let rows = reference_identity(RuntimeOp::ProcessList);
        let replacement = facts.registry_references[&reference_identity(RuntimeOp::JsonEncode)].native_authority().unwrap();
        let crate::sema::inference::NativeAuthority::Single(contract) = facts.registry_references[&rows].native_authority().unwrap() else { panic!("the producer reference retains one native contract"); };
        assert_eq!(facts.graph.native_contract(contract).unwrap().instance.effect_roots,
            [EffectSummary::Closed(EffectSet::PROCESS), EffectSummary::Closed(EffectSet::EMPTY)]);
        let signature = facts.graph.native_contract(contract).unwrap().instance.ty;
        let reference = facts.registry_references.get_mut(&rows).unwrap();
        let super::registry_boundaries::RegistryReferenceContract::Native { callable, authority } = &mut reference.contract else { panic!("the source reference preserves its native authority"); };
        assert_ne!(*callable, signature);
        if replace_contract { *authority = replacement; }
        else { *callable = signature; }
        assert!(facts.validate().is_err(), "source native authority must agree with its exact monotype receipt");
    }
}

#[test]
fn native_producer_callable_forwarding_checks_later_consumer_permissions() {
    let definitions = "pure retain(value) { value }\nproc opened(factory) [process] { factory() }\nlet services = {rows: retain(process.list)}\n";
    for (permissions, accepted) in [("process", true), ("", false)] {
        let source = format!("{definitions}proc consumed(rows) [{permissions}] {{ rows.collect() }}\nlet values = consumed(opened(services.rows)?)\n");
        if accepted { accepted_source(&source); }
        else {
            let checked = checked_source(&source);
            assert!(checked.diagnostics.iter().any(|diagnostic|
                diagnostic.code.as_deref() == Some("check.effect-violation")), "{source}: {:?}", checked.diagnostics);
        }
    }
}

#[test]
fn native_and_user_callable_conditionals_preserve_guards_in_both_orders() {
    let definitions = "pure fallback(value: Any, pretty: Bool = false) -> Result[Str] { Ok(\"fallback\") }\n";
    for (first, second) in [("json.encode", "fallback"), ("fallback", "json.encode")] {
        let selection = format!("{definitions}pure choose(select: Bool) {{ if select {{ ({first}) }} else {{ ({second}) }} }}\nlet encode = choose(true)\n");
        let checked = accepted_source(&format!("{selection}let good: Result[Str] = encode(value: {{valid: true}})\n"));
        native_receipts(&checked, RuntimeOp::JsonEncode);
        rejected_source(&format!("{selection}let bad = encode(value: b\"raw\")\n"));
    }
}

#[test]
fn native_callback_parameter_cannot_acquire_independent_polymorphic_results() {
    accepted_source("let empty = map.empty\nlet first: Map[Int] = empty()\nlet second: Map[Str] = empty()\n");
    rejected_source("pure incompatible(factory) { let first: Map[Int] = factory(); let second: Map[Str] = factory(); first }\nlet bad = incompatible(map.empty)\n");
    rejected_source("var empty = map.empty\nlet alias = empty\nlet first: Map[Int] = alias()\nlet second: Map[Str] = empty()\n");
}

#[test]
fn native_callable_list_projection_preserves_guards_and_producer_permissions() {
    let definitions = "pure retain(value) { value }\nlet callbacks = retain([json.encode])\n";
    let checked = accepted_source(&format!("{definitions}let good: Result[Str] = callbacks[0]({{value: 1}})\n"));
    native_receipts(&checked, RuntimeOp::JsonEncode);
    rejected_source(&format!("{definitions}let bad = callbacks[0](b\"raw\")\n"));

    let definitions = "pure retain(value) { value }\nlet factories = retain([process.list])\n";
    let checked = accepted_source(&format!("{definitions}let rows = factories[0]()\n"));
    native_receipts(&checked, RuntimeOp::ProcessList);
    assert!(checked.solved.binding_producers.values().any(|profile| profile.iter().any(|(path, effects)|
        path.0 == [ProducerPathComponent::ResultSuccess]
            && effects.pull == EffectSummary::Closed(EffectSet::PROCESS)
            && effects.close == EffectSummary::Closed(EffectSet::EMPTY))));
    rejected_source(&format!("{definitions}proc denied() [] {{ factories[0]() }}\nlet bad = denied()\n"));
}

#[test]
fn native_and_user_producer_conditionals_preserve_permissions_in_both_orders() {
    let definitions = "proc fallback() [process] { process.list() }\n";
    for (first, second) in [("process.list", "fallback"), ("fallback", "process.list")] {
        let selection = format!("{definitions}pure choose(select: Bool) {{ if select {{ ({first}) }} else {{ ({second}) }} }}\n");
        let opened = "proc opened(factory) [process] { factory() }\n";
        let checked = accepted_source(&format!("{selection}{opened}proc consumed(rows) [process] {{ rows.collect() }}\nlet values = consumed(opened(choose(true))?)\n"));
        native_receipts(&checked, RuntimeOp::ProcessList);
        let denied = checked_source(&format!("{selection}{opened}proc consumed(rows) [] {{ rows.collect() }}\nlet bad = consumed(opened(choose(true))?)\n"));
        assert!(denied.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")),
            "{selection}: {:?}", denied.diagnostics);
    }
}

#[test]
fn native_stage_callback_alias_retains_original_json_guards() {
    let definitions = "let encoder = json.encode\npure mapped(values) { values |> map(encoder) }\n";
    let checked = accepted_source(&format!("{definitions}let good: List[Result[Str]] = mapped([1])\n"));
    native_receipts(&checked, RuntimeOp::JsonEncode);
    rejected_source(&format!("{definitions}let bad = mapped([Path(\"item\")])\n"));
}

#[test]
fn native_stage_callback_result_preserves_nested_producer_roles() {
    let definitions = "let listing = process.list\nproc rows(value) [process] { listing() }\n";
    let checked = accepted_source(&format!("{definitions}proc mapped(values) [process] {{ values |> map(rows) }}\nlet results = mapped([1])\n"));
    native_receipts(&checked, RuntimeOp::ProcessList);
    assert!(checked.solved.binding_producers.values().any(|profile| profile.iter().any(|(path, effects)|
        path.0 == [ProducerPathComponent::ListItem, ProducerPathComponent::ResultSuccess]
            && effects.pull == EffectSummary::Closed(EffectSet::PROCESS)
            && effects.close == EffectSummary::Closed(EffectSet::EMPTY))),
        "mapping a native factory result must retain its nested lifecycle roles");
    let denied = checked_source(&format!("{definitions}proc mapped(values) [] {{ values |> map(rows) }}\nlet bad = mapped([1])\n"));
    assert!(denied.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")),
        "{:?}", denied.diagnostics);
    rejected_source("let listing = process.list\nlet bad = [1] |> map(listing)\n");
}

#[test]
fn native_stage_choice_selects_the_one_item_member_and_retains_its_roles() {
    for (name, zero_operation, item_operation, pull) in [
        ("ports", RuntimeOp::ProcessPorts, RuntimeOp::ProcessPortsForPid, EffectSet::EMPTY),
        ("threads", RuntimeOp::ProcessThreads, RuntimeOp::ProcessThreads, EffectSet::PROCESS),
    ] {
        let direct = accepted_source(&format!("let values = [7] |> map(process.{name})\n"));
        native_receipts(&direct, item_operation);
        let prefix = format!("let inspect = process.{name}\n");
        let checked = accepted_source(&format!("{prefix}let all = inspect()\nlet one = inspect(7)\nproc mapped(values) [process] {{ values |> map(inspect) }}\nlet values = mapped([7])\n"));
        native_receipts(&checked, zero_operation);
        native_receipts(&checked, item_operation);
        let graph = &checked.solved.graph;
        let reference = checked.solved.registry_references.values().next().unwrap();
        let callable = graph.callable_signature(reference.value_type()).unwrap();
        let TypeNode::CallableChoice(members) = graph.node(graph.resolved(callable).unwrap()).unwrap() else { panic!("different arities remain separate complete monotypes"); };
        assert_eq!(members.len(), 2);
        let Some(super::StageCallback::Protocol { operation, formal_slot, expression, .. }) = checked.solved.stage_operations.values().next().unwrap().callback else { panic!("stage retains its original operation and callback slot"); };
        let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation).unwrap() else { panic!("stage projection belongs to an operation"); };
        let original = graph.operation_call(call).unwrap().arguments[formal_slot - 1].unwrap();
        let original_source = checked.solved.expressions.iter().find(|(identity, _)| identity.expression == expression).unwrap().1;
        assert_eq!(graph.resolved(original).unwrap(), graph.resolved(*original_source).unwrap());
        let requirement = graph.candidate_callback_invocation(operation, formal_slot).unwrap().or_else(|| {
            checked.solved.calls.values().flat_map(|call| call.requirements.iter().copied()).find_map(|requirement|
                graph.candidate_callback_invocation(requirement, formal_slot).ok().flatten())
        }).expect("actual source item selects an original callback invocation");
        let evidence = graph.invocation_evidence(requirement).unwrap().unwrap();
        let (signature, binding, timing) = evidence.unique_plan().expect("one native family selects one original member");
        let TypeNode::Arrow(signature) = graph.node(graph.resolved(signature).unwrap()).unwrap() else { panic!("only the selected one-item member supplies invocation evidence"); };
        assert_eq!(signature.params.len(), 1);
        assert_eq!(binding.supplied_slots, [0]);
        assert!(binding.default_slots.is_empty());
        assert_eq!(timing, InvocationDefaultTiming::AtCall);
        assert!(checked.solved.binding_producers.values().any(|profile| profile.iter().any(|(path, effects)|
            path.0 == [ProducerPathComponent::ListItem, ProducerPathComponent::ResultSuccess]
                && effects.pull == EffectSummary::Closed(pull)
                && effects.close == EffectSummary::Closed(EffectSet::EMPTY))),
            "{name}: selected member must retain its exact nested producer roles");
        rejected_source(&format!("{prefix}let bad = inspect(7, 8)\n"));
        rejected_source(&format!("{prefix}proc mapped(values) [process] {{ values |> map(inspect) }}\nlet bad = mapped([true])\n"));
    }
}

#[test]
fn native_stage_mixed_kind_choice_keeps_both_protocols_until_actual_selection() {
    accepted_source("let executable = fs.executable\npure modes() { [1] |> map(executable) }\n");
    accepted_source("let executable = fs.executable\nproc files() [fs] { [Path(\"item\")] |> map(executable) }\n");
    let definitions = "let executable = fs.executable\nproc mapped(values) [fs] { values |> map(executable) }\n";
    let checked = accepted_source(&format!("{definitions}let modes: List[Bool] = mapped([1])\nlet files: List[Result[Bool]] = mapped([Path(\"item\")])\n"));
    native_receipts(&checked, RuntimeOp::FsExecutable);
    let graph = &checked.solved.graph;
    let stage = checked.solved.stage_operations.values().next().unwrap();
    let Some(super::StageCallback::Protocol { operation, formal_slot, .. }) = stage.callback else { panic!("mixed kind callback retains a candidate projection"); };
    let RequirementTemplate::Operation { family, .. } = graph.requirement_template(operation).unwrap() else { panic!() };
    let mut kinds = std::collections::BTreeSet::new();
    for &candidate in graph.family(family).unwrap() {
        let super::SolvedOperationAuthority::Stage(metadata) = checked.solved.operation_catalog.candidate(graph, candidate).unwrap() else { panic!() };
        kinds.insert(format!("{:?}", metadata.form.callback_kind.unwrap()));
        assert!(metadata.form.callback_protocol);
        assert_eq!(graph.candidate(candidate).unwrap().argument_relations[formal_slot], crate::sema::inference::ArgumentRelation::InvocationProtocol);
    }
    assert_eq!(kinds, ["Pure".to_string(), "Proc".to_string()].into_iter().collect());
    let mut selected = std::collections::BTreeSet::new();
    for requirement in checked.solved.calls.values().flat_map(|call| call.requirements.iter().copied()) {
        if let Some(invocation) = graph.candidate_callback_invocation(requirement, formal_slot).ok().flatten() {
            if let Some(evidence) = graph.invocation_evidence(invocation).unwrap() {
                let (signature, binding, timing) = evidence.unique_plan().expect("one native family selects its original item member");
                let TypeNode::Arrow(arrow) = graph.node(graph.resolved(signature).unwrap()).unwrap() else { panic!() };
                selected.insert(format!("{:?}", arrow.kind));
                let expected = if arrow.kind == crate::sema::inference::CallableKind::Pure { EffectSet::EMPTY } else { EffectSet::FS };
                assert_eq!(graph.resolved_effect_summary(evidence.effects).unwrap(), EffectSummary::Closed(expected));
                assert_eq!(binding.supplied_slots, [0]);
                assert!(binding.default_slots.is_empty());
                assert_eq!(timing, InvocationDefaultTiming::AtCall);
            }
        }
    }
    assert_eq!(selected, kinds, "each source instance selects the original member kind");
    accepted_source("let executable = fs.executable\npure modes(values) { values |> map(executable) }\nlet good: List[Bool] = modes([1])\n");
    rejected_source("let executable = fs.executable\npure files(values) { values |> map(executable) }\nlet bad = files([Path(\"item\")])\n");
    let denied = checked_source("let executable = fs.executable\nproc files(values) [] { values |> map(executable) }\nlet bad = files([Path(\"item\")])\n");
    assert!(denied.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", denied.diagnostics);
    rejected_source(&format!("{definitions}let bad = mapped([true])\n"));
}

#[test]
fn native_stage_conditional_hash_choice_keeps_each_original_kind_and_effect() {
    for (left, right) in [("hash.md5", "hash.sha256"), ("hash.sha256", "hash.md5")] {
        let definitions = format!("pure choose(flag: Bool) {{ if flag {{ ({left}) }} else {{ ({right}) }} }}\nlet digest = choose(true)\nproc mapped(values) [fs] {{ values |> map(digest) }}\n");
        let checked = accepted_source(&format!("{definitions}let data: List[Digest] = mapped([b\"input\"])\nlet files: List[Result[Digest]] = mapped([Path(\"item\")])\n"));
        native_receipts(&checked, RuntimeOp::HashMd5);
        native_receipts(&checked, RuntimeOp::HashSha256);
        let graph = &checked.solved.graph;
        let mut effects = std::collections::BTreeSet::new();
        for call in checked.solved.calls.values() {
            for &requirement in &call.requirements {
                if let Some(callback) = graph.candidate_callback_invocation(requirement, 1).ok().flatten() {
                    let evidence = graph.invocation_evidence(callback).unwrap().unwrap();
                    let crate::sema::inference::InvocationPlan::All { branches } = &evidence.plan else { panic!("conditional choice preserves each original authority plan"); };
                    assert_eq!(branches.len(), 2);
                    assert!(evidence.unique_plan().is_none());
                    for branch in branches {
                        assert!(matches!(branch.authority, crate::sema::inference::CallableAuthority::Native { .. }));
                        let TypeNode::Arrow(arrow) = graph.node(graph.resolved(branch.signature).unwrap()).unwrap() else { panic!() };
                        let expected = if arrow.kind == crate::sema::inference::CallableKind::Pure { EffectSet::EMPTY } else { EffectSet::FS };
                        assert_eq!(graph.resolved_effect_summary(branch.effects).unwrap(), EffectSummary::Closed(expected));
                        assert_eq!(branch.binding.supplied_slots, [0]);
                        assert!(branch.binding.default_slots.is_empty());
                        assert_eq!(branch.timing, InvocationDefaultTiming::AtCall);
                        effects.insert(format!("{:?}:{:?}", arrow.kind, graph.resolved_effect_summary(branch.effects).unwrap()));
                    }
                    assert_eq!(evidence.native_alternatives.len(), 2, "conditional invocation must prove both original authorities");
                    let stage = graph.candidate_evidence(requirement).unwrap().unwrap();
                    assert_eq!(graph.resolved_effect_summary(stage.effects).unwrap(), EffectSummary::Closed(EffectSet::EMPTY));
                    let callback_effects = graph.resolved_effect_summary(evidence.effects).unwrap();
                    for &(role, index) in &graph.candidate(stage.candidate).unwrap().output_effect_roles {
                        let expected = if role == crate::sema::inference::ProducerRole::Pull { callback_effects } else { EffectSummary::Closed(EffectSet::EMPTY) };
                        assert_eq!(graph.resolved_effect_summary(stage.effect_roots[index as usize]).unwrap(), expected,
                            "callback invocation belongs to stage pulls, not producer creation or cleanup");
                    }
                }
            }
        }
        assert!(effects.iter().any(|effect| effect.starts_with("Pure:")));
        assert!(effects.iter().any(|effect| effect.starts_with("Proc:")));
        accepted_source(&format!("pure choose(flag: Bool) {{ if flag {{ ({left}) }} else {{ ({right}) }} }}\nlet digest = choose(true)\npure mapped(values) {{ values |> map(digest) }}\nlet good: List[Digest] = mapped([b\"input\"])\n"));
        rejected_source(&format!("pure choose(flag: Bool) {{ if flag {{ ({left}) }} else {{ ({right}) }} }}\nlet digest = choose(true)\npure mapped(values) {{ values |> map(digest) }}\nlet bad = mapped([Path(\"item\")])\n"));
        let denied = checked_source(&format!("pure choose(flag: Bool) {{ if flag {{ ({left}) }} else {{ ({right}) }} }}\nlet digest = choose(true)\nproc mapped(values) [] {{ values |> map(digest) }}\nlet bad = mapped([Path(\"item\")])\n"));
        assert!(denied.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", denied.diagnostics);
        rejected_source(&format!("{definitions}let bad = mapped([true])\n"));
    }
}

#[test]
fn stage_conditional_user_callbacks_keep_branch_kinds_and_aggregate_proc_permissions() {
    for (left, right) in [("unchanged", "delayed"), ("delayed", "unchanged")] {
        let definitions = format!("pure unchanged(value: Int) -> Int {{ value }}\nproc delayed(value: Int) [time] -> Int {{ let _ = time.now(); value }}\npure choose(flag: Bool) {{ if flag {{ ({left}) }} else {{ ({right}) }} }}\nlet callback = choose(true)\n");
        let checked = accepted_source(&format!("{definitions}proc mapped(values) [time] {{ values |> map(callback) }}\nlet good: List[Int] = mapped([1])\n"));
        let graph = &checked.solved.graph;
        let stage = checked.solved.stage_operations.values().next().unwrap();
        let Some(super::StageCallback::Protocol { operation, formal_slot, .. }) = stage.callback else { panic!("conditional callbacks retain the original operation projection"); };
        let invocation = graph.candidate_callback_invocation(operation, formal_slot).unwrap().or_else(||
            checked.solved.calls.values().flat_map(|call| call.requirements.iter().copied()).find_map(|requirement|
                graph.candidate_callback_invocation(requirement, formal_slot).ok().flatten())).unwrap();
        let evidence = graph.invocation_evidence(invocation).unwrap().unwrap();
        let RequirementTemplate::CallableInvocation { call } = graph.requirement_template(invocation).unwrap() else { panic!() };
        let actual = graph.invocation_call(call).unwrap();
        assert_eq!(actual.domain, crate::sema::inference::CallableDomain::Exact(crate::sema::inference::CallableKind::Proc));
        let signature = graph.callable_signature(actual.callable).unwrap();
        assert!(matches!(graph.node(graph.resolved(signature).unwrap()).unwrap(), TypeNode::CallableChoice(members) if members.len() == 2));
        let crate::sema::inference::InvocationPlan::All { branches } = &evidence.plan else { panic!("the aggregate protocol must retain both original user plans"); };
        assert_eq!(branches.len(), 2);
        assert!(evidence.unique_plan().is_none());
        assert_eq!(graph.resolved_effect_summary(evidence.effects).unwrap(), EffectSummary::Closed(EffectSet::TIME));
        let mut kinds = std::collections::BTreeSet::new();
        for branch in branches {
            let crate::sema::inference::CallableAuthority::User { origin, .. } = branch.authority else { panic!("each plan retains its source declaration origin"); };
            assert!(matches!(graph.node(graph.resolved(origin).unwrap()).unwrap(), TypeNode::Arrow(_)));
            let TypeNode::Arrow(arrow) = graph.node(graph.resolved(branch.signature).unwrap()).unwrap() else { panic!() };
            kinds.insert(format!("{:?}", arrow.kind));
            let effects = if arrow.kind == crate::sema::inference::CallableKind::Pure { EffectSet::EMPTY } else { EffectSet::TIME };
            assert_eq!(graph.resolved_effect_summary(branch.effects).unwrap(), EffectSummary::Closed(effects));
            assert_eq!(branch.binding.supplied_slots, [0]);
            assert!(branch.binding.default_slots.is_empty());
            assert_eq!(branch.timing, InvocationDefaultTiming::AtCall);
        }
        assert_eq!(kinds, ["Pure".to_string(), "Proc".to_string()].into_iter().collect());
        let denied = checked_source(&format!("{definitions}proc mapped(values) [] {{ values |> map(callback) }}\nlet bad = mapped([1])\n"));
        assert!(denied.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", denied.diagnostics);
        rejected_source(&format!("{definitions}pure mapped(values) {{ values |> map(callback) }}\nlet bad = mapped([1])\n"));
        rejected_source(&format!("{definitions}proc mapped(values) [time] {{ values |> map(callback) }}\nlet bad = mapped([true])\n"));
    }
}
