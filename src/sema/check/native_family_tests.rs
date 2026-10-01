use super::{CheckOutput, Checker, SolvedOperationAuthority};
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

fn rejected_source(source: &str) {
    let checked = checked_source(source);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref().is_some_and(|code| code.starts_with("check."))),
        "{source}: {:?}", checked.diagnostics);
}

fn selected_arities(checked: &CheckOutput, operation: RuntimeOp) -> std::collections::BTreeSet<usize> {
    let graph = &checked.solved.graph;
    let mut arities = std::collections::BTreeSet::new();
    for requirement in checked.solved.invocations.values().map(|invocation| invocation.requirement)
        .chain(checked.solved.calls.values().flat_map(|call| call.requirements.iter().copied())) {
        let Some(invocation) = graph.invocation_evidence(requirement).unwrap() else { continue; };
        for native in &invocation.native_alternatives {
            let Some(evidence) = graph.candidate_evidence(native.operation).unwrap() else {
                assert!(checked.solved.declarations.values().any(|declaration| declaration.source_requirements.contains(&requirement)));
                continue;
            };
            let SolvedOperationAuthority::Registry(metadata) = checked.solved.operation_catalog.candidate(graph, evidence.candidate).unwrap() else { panic!("native family retains registry authority") };
            if metadata.operation != operation { continue; }
            let TypeNode::Arrow(signature) = graph.node(graph.resolved(evidence.signature).unwrap()).unwrap() else { panic!("selection retains the original member Arrow") };
            arities.insert(signature.params.len());
            let RequirementTemplate::Operation { family, call } = graph.requirement_template(native.operation).unwrap() else { panic!("selection retains its child operation") };
            assert!(graph.family(family).unwrap().contains(&evidence.candidate));
            if let crate::sema::inference::OperationBinding::Invocation(original) = graph.operation_call(call).unwrap().binding {
                let original = graph.invocation_call(original).unwrap();
                let binding = evidence.binding.as_ref().expect("member selection retains its original argument binding");
                assert_eq!(binding.supplied_slots.len(), original.arguments.len());
                assert_eq!(evidence.actual_arguments.len(), signature.params.len());
                for (argument, &slot) in original.arguments.iter().zip(&binding.supplied_slots) {
                    assert_eq!(graph.resolved(argument.ty).unwrap(), graph.resolved(evidence.actual_arguments[slot].unwrap()).unwrap());
                }
                for &slot in &binding.default_slots { assert!(evidence.actual_arguments[slot].is_none()); }
            }
        }
    }
    assert!(!arities.is_empty(), "native calls retain actual selected member receipts");
    arities
}

#[test]
fn native_process_family_retains_zero_and_one_argument_producer_members() {
    for reference in ["process.ports", "process.threads"] {
        let source = format!("pure retain(value) {{ value }}\nlet query = retain({reference})\nproc all_entries() [process] {{ query() }}\nproc one_entry(pid: Int) [process] {{ query(pid: pid) }}\n");
        let checked = accepted_source(&source);
        let graph = &checked.solved.graph;
        let retained = checked.solved.registry_references.values().next().unwrap();
        assert_eq!(retained.candidates(graph).unwrap().len(), 2);
        let mut arities = std::collections::BTreeSet::new();
        for candidate in retained.candidates(graph).unwrap() {
            let SolvedOperationAuthority::Registry(metadata) = checked.solved.operation_catalog.candidate(graph, candidate).unwrap() else { panic!() };
            arities.extend(selected_arities(&checked, metadata.operation));
        }
        assert_eq!(arities, [0, 1].into_iter().collect());
        rejected_source(&format!("let query = {reference}\nproc forbidden() [] {{ query() }}\n"));
        rejected_source(&format!("let query = {reference}\nproc wrong() [process] {{ query(pid: \"wrong\") }}\n"));
    }
}

#[test]
fn native_hash_family_retains_each_argument_kind_result_and_permission() {
    for reference in ["hash.md5", "hash.sha1", "hash.sha256", "hash.sha512"] {
        let source = format!("let digest = {reference}\npure from_bytes(data: Bytes) -> Digest {{ digest(data: data) }}\nproc from_file(file: Path) [fs] -> Result[Digest] {{ digest(path: file) }}\n");
        let checked = accepted_source(&source);
        let graph = &checked.solved.graph;
        let mut heads = std::collections::BTreeSet::new();
        for reference in checked.solved.registry_references.values() {
            for candidate in reference.candidates(graph).unwrap() {
                let template = graph.candidate(candidate).unwrap();
                let TypeNode::Arrow(arrow) = graph.node(graph.scheme(template.scheme).unwrap().body).unwrap() else { panic!() };
                heads.insert(matches!(graph.node(arrow.result).unwrap(), TypeNode::Result(_, _)));
            }
        }
        assert_eq!(heads, [false, true].into_iter().collect());
        rejected_source(&format!("let digest = {reference}\npure forbidden(file: Path) {{ digest(path: file) }}\n"));
        rejected_source(&format!("let digest = {reference}\nproc forbidden(file: Path) [] {{ digest(path: file) }}\n"));
        rejected_source(&format!("let digest = {reference}\npure wrong() {{ digest(data: \"wrong\") }}\n"));
    }
}

#[test]
fn native_json_get_family_retains_fallback_dependent_result_and_original_args() {
    let source = "let lookup = json.get\npure required(value: Any, keys: List[Any]) -> Result[Any] { lookup(value, keys) }\npure fallback(value: Any, keys: List[Any]) -> Any { lookup(value: value, path: keys, fallback: false) }\n";
    let checked = accepted_source(source);
    assert_eq!(selected_arities(&checked, RuntimeOp::JsonGet), [2, 3].into_iter().collect());
    rejected_source("let lookup = json.get\npure wrong(value: Any) { lookup(value) }\n");
    rejected_source("let lookup = json.get\npure wrong(value: Any) { lookup(value, 7) }\n");
}

#[test]
fn native_executable_family_keeps_mode_pure_and_path_effectful_results() {
    let source = "let executable = fs.executable\npure from_mode(value: Int) -> Bool { executable(mode: value) }\nproc from_path(value: Path) [fs] -> Result[Bool] { executable(path: value) }\n";
    let checked = accepted_source(source);
    assert_eq!(selected_arities(&checked, RuntimeOp::FsExecutable), [1].into_iter().collect());
    let graph = &checked.solved.graph;
    assert!(checked.solved.expressions.values().any(|ty| matches!(graph.node(graph.resolved(*ty).unwrap()).unwrap(), TypeNode::Atom(Atom::Bool))));
    rejected_source("let executable = fs.executable\npure forbidden(value: Path) { executable(path: value) }\n");
    rejected_source("let executable = fs.executable\npure wrong() { executable(mode: true) }\n");
}
