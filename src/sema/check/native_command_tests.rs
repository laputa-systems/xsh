use super::{CheckOutput, Checker};
use crate::modules::RuntimeOp;
use crate::sema::inference::{Atom, RequirementTemplate, TypeNode};
use crate::source::SourceId;
use crate::syntax::parser::Parser;

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

fn native_command_receipts(checked: &CheckOutput) -> Vec<crate::sema::inference::RequirementId> {
    let graph = &checked.solved.graph;
    let mut receipts = std::collections::BTreeSet::new();
    for requirement in checked.solved.invocations.values().map(|invocation| invocation.requirement)
        .chain(checked.solved.calls.values().flat_map(|call| call.requirements.iter().copied())) {
        let Some(invocation) = graph.invocation_evidence(requirement).unwrap() else { continue; };
        for native in &invocation.native_alternatives {
            let Some(evidence) = graph.candidate_evidence(native.operation).unwrap() else {
                assert!(checked.solved.declarations.values().any(|declaration| declaration.source_requirements.contains(&requirement)),
                    "only a retained generic source obligation may lack selected native evidence");
                continue;
            };
            let super::SolvedOperationAuthority::Registry(metadata) = checked.solved.operation_catalog.candidate(graph, evidence.candidate).unwrap() else { panic!("native command must retain registry authority"); };
            if metadata.operation == RuntimeOp::ProcessCommandArgv { receipts.insert(native.operation); }
        }
    }
    assert!(!receipts.is_empty(), "a native command call must choose one canonical overload");
    receipts.into_iter().collect()
}

#[test]
fn native_command_captured_factory_retains_pending_original_argument_guards() {
    let declarations = "let make = process.command_argv\npure build(target, arguments) { make(target, arguments) }\n";
    let checked = accepted_source(&format!("{declarations}let first: Command = build(\"tool\", [Path(\"one\")])\nlet second: Command = build(Path(\"tool\"), [\"two\"])\n"));
    native_command_receipts(&checked);
    let graph = &checked.solved.graph;
    let pending: Vec<_> = checked.solved.invocations.values().filter(|invocation|
        invocation.caller.is_some() && graph.invocation_evidence(invocation.requirement).unwrap().is_none()).collect();
    assert_eq!(pending.len(), 1, "the generic body retains its unresolved native choice");
    let RequirementTemplate::CallableInvocation { call } = graph.requirement_template(pending[0].requirement).unwrap() else { panic!("expected original invocation"); };
    let original = graph.invocation_call(call).unwrap();
    let children = graph.native_invocation_children(pending[0].requirement).unwrap();
    assert_eq!(children.len(), 1);
    let RequirementTemplate::Operation { call, .. } = graph.requirement_template(children[0].operation).unwrap() else { panic!("expected canonical native child"); };
    let child = graph.operation_call(call).unwrap();
    assert!(matches!(child.binding, crate::sema::inference::OperationBinding::Slots));
    assert_eq!(child.arguments[..2], [Some(original.arguments[0].ty), Some(original.arguments[1].ty)]);
    assert!(child.arguments[2..].iter().all(Option::is_none));
    assert!(graph.candidate_evidence(children[0].operation).unwrap().is_none());
    let literal = "let make = process.command_argv\npure build(item) { make(\"tool\", [item]) }\n";
    let checked = accepted_source(&format!("{literal}let valid: Command = build(Path(\"item\"))\n"));
    native_command_receipts(&checked);
    assert!(checked.solved.registry_boundaries.values().any(|boundary| matches!(&boundary.kind,
        super::registry_boundaries::RegistryBoundaryKind::CommandArguments { argv_children } if argv_children.len() == 1)));
    for source in [format!("{declarations}let invalid = build(7, [\"item\"])\n"),
        format!("{declarations}let invalid = build(\"tool\", [7])\n"),
        format!("{literal}let invalid = build(7)\n")] {
        let checked = checked_source(&source);
        assert!(!checked.diagnostics.is_empty(), "{source}");
    }
}

#[test]
fn native_command_callable_chooses_one_original_domain_after_forwarding() {
    let declarations = "pure build(factory, target, arguments) { factory(argv: arguments, target: target) }\nlet make = process.command_argv\n";
    for (target, arguments, target_atom, item_atom) in [
        ("\"tool\"", "[\"item\"]", Atom::Str, Atom::Str),
        ("\"tool\"", "[Path(\"item\")]", Atom::Str, Atom::Path),
        ("Path(\"tool\")", "[\"item\"]", Atom::Path, Atom::Str),
        ("Path(\"tool\")", "[Path(\"item\")]", Atom::Path, Atom::Path),
    ] {
        let source = format!("{declarations}let recipe: Command = build(make, {target}, {arguments})\n");
        let checked = accepted_source(&source);
        for requirement in native_command_receipts(&checked) {
            let graph = &checked.solved.graph;
            let RequirementTemplate::Operation { call, .. } = graph.requirement_template(requirement).unwrap() else { panic!("native selection must retain its operation call"); };
            let call = graph.operation_call(call).unwrap();
            let actuals = &graph.candidate_evidence(requirement).unwrap().unwrap().actual_arguments;
            assert!(call.mono_authority.is_some());
            assert!(matches!(graph.node(graph.resolved(actuals[0].unwrap()).unwrap()).unwrap(), TypeNode::Atom(atom) if *atom == target_atom));
            let TypeNode::List(item) = graph.node(graph.resolved(actuals[1].unwrap()).unwrap()).unwrap() else { panic!("original argv remains a list"); };
            assert!(matches!(graph.node(graph.resolved(*item).unwrap()).unwrap(), TypeNode::Atom(atom) if *atom == item_atom));
        }
    }
    accepted_source(&format!("{declarations}let first: Command = build(make, \"tool\", [Path(\"one\")])\nlet second: Command = build(make, Path(\"tool\"), [\"two\"])\n"));
}

#[test]
fn native_command_callable_retains_required_and_omitted_stdin_alternatives() {
    for stdin in ["", ", stdin: Path(\"input\")", ", stdin: b\"input\""] {
        let source = format!("let make = process.command_argv\nlet recipe: Command = make(argv: [\"item\"], target: Path(\"tool\"){stdin})\n");
        let checked = accepted_source(&source);
        assert_eq!(native_command_receipts(&checked).len(), 1);
    }
}

#[test]
fn native_command_callable_retains_mixed_literal_children_and_dynamic_admission() {
    let source = "let make = process.command_argv\nlet recipe: Command = make(Path(\"tool\"), [\"text\", Path(\"item\")])\n";
    let checked = accepted_source(source);
    native_command_receipts(&checked);
    assert!(checked.solved.registry_boundaries.values().any(|boundary| matches!(&boundary.kind,
        super::registry_boundaries::RegistryBoundaryKind::CommandArguments { argv_children } if argv_children.len() == 2)),
        "mixed literal argv needs its original child validation proofs");
    accepted_source("let make = process.command_argv\npure dynamic(target: Any, arguments: Any) -> Command { make(target, arguments) }\n");
}

#[test]
fn native_command_callable_keeps_explicit_path_optional_argument_types() {
    for callee in ["process.command_argv", "make"] {
        let source = format!("let make = process.command_argv\nlet recipe: Command = {callee}(\"tool\", [\"item\"], cwd: Path(\"directory\"), stdin: Path(\"input\"), stdout: Path(\"output\"), stderr: Path(\"error\"))\n");
        accepted_source(&source);
        for slot in ["cwd", "stdin", "stdout", "stderr"] {
            let source = format!("let make = process.command_argv\nlet recipe: Command = {callee}(\"tool\", [\"item\"], {slot}: \"untyped path\")\n");
            let checked = checked_source(&source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))),
                "{source}: {:?}", checked.diagnostics);
        }
    }
}

#[test]
fn native_command_callable_rejects_wrong_actual_domains_and_binding_shapes() {
    let declarations = "pure build(factory, target, arguments) { factory(target, arguments) }\nlet make = process.command_argv\n";
    accepted_source(&format!("{declarations}let valid: Command = build(make, \"tool\", [\"item\"])\n"));
    for invocation in [
        "build(make, 7, [\"item\"])", "build(make, \"tool\", [7])", "build(make, \"tool\", [[\"item\"]])",
        "build(make, \"tool\", b\"raw\")", "make()", "make(\"tool\")", "make(target: \"tool\", argv: [], unknown: true)",
        "make(\"tool\", [], stdin: 7)", "make(\"tool\", [], detach: \"yes\")",
    ] {
        let source = format!("{declarations}let invalid = {invocation}\n");
        let checked = checked_source(&source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))), "{source}: {:?}", checked.diagnostics);
    }
}
