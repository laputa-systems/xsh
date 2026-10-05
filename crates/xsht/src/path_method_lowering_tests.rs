//! Every `Path` method in the registry must be executable, not only
//! checkable. A method that the checker accepts but lowering has no route
//! for fails with "cannot be compiled yet" in the first program that calls
//! it, so this test calls each overload once and lowers the call.

use xsh::execution::evaluator::Evaluator;
use xsh::frontend::check::Checker;
use xsh::frontend::source::SourceMap;
use xsh::frontend::syntax::parser::Parser;
use xsh_registry::signature::{MethodReceiver, api_spec};
use xsh_registry::types::Type;

/// A literal of a parameter type. A new parameter type needs a line here.
fn argument(ty: &Type) -> &'static str {
    match ty {
        Type::Path => "p\"x\"",
        Type::Str => "\"x\"",
        Type::Bytes => "b\"x\"",
        Type::Int => "1",
        Type::Bool => "true",
        Type::List(item) if **item == Type::Str => "[\"x\"]",
        other => panic!("no literal for a `Path` method parameter of type {other:?}"),
    }
}

/// The diagnostics of checking and lowering one program.
fn check_and_lower(source: &str) -> Vec<String> {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("path-method.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    let mut diagnostics = parsed.diagnostics.clone();
    if diagnostics.is_empty() {
        diagnostics = Checker::check_arena(&parsed.arena, source).diagnostics;
    }
    if diagnostics.is_empty() {
        diagnostics = Evaluator::compact_lowerability_diagnostics(
            &parsed.arena,
            source_id,
            sources,
            Vec::new(),
            "path-method".to_owned(),
        );
    }
    diagnostics
        .into_iter()
        .map(|diagnostic| format!("{:?}: {}", diagnostic.code, diagnostic.message))
        .collect()
}

#[test]
fn every_registry_path_method_lowers() {
    let receiver = api_spec()
        .methods
        .iter()
        .find(|entry| entry.receiver == MethodReceiver::Path)
        .expect("the registry has Path methods");
    let mut calls = 0;
    for method in &receiver.methods {
        for overload in &method.overloads {
            // Required parameters positionally, then every parameter, so a
            // defaulted one is lowered both absent and present.
            let required = overload
                .sig
                .params
                .iter()
                .filter(|param| !param.defaulted)
                .count();
            for arity in [required, overload.sig.params.len()] {
                let arguments = overload.sig.params[..arity]
                    .iter()
                    .map(|param| argument(&param.ty))
                    .collect::<Vec<_>>()
                    .join(", ");
                // A plain call and a null-safe one are lowered separately.
                for (parameter, access) in [("Path", "."), ("Path?", "?.")] {
                    let source = format!(
                        "proc probe(target: {parameter}) {{\n  let _ = target{access}{}({arguments})\n}}\n",
                        method.name
                    );
                    let diagnostics = check_and_lower(&source);
                    assert!(
                        diagnostics.is_empty(),
                        "`Path.{}` does not lower:\n{source}{diagnostics:#?}",
                        method.name
                    );
                    calls += 1;
                }
            }
        }
    }
    assert!(calls > 160, "only {calls} calls were lowered");
}

/// `is_empty` is declared once per receiver in the registry and routed by
/// name in lowering, so each receiver needs its own route.
#[test]
fn is_empty_lowers_for_every_receiver_that_declares_it() {
    let mut receivers = 0;
    for entry in &api_spec().methods {
        if !entry.methods.iter().any(|method| method.name == "is_empty") {
            continue;
        }
        let ty = match entry.receiver {
            MethodReceiver::Str => "Str",
            MethodReceiver::Bytes => "Bytes",
            MethodReceiver::List => "List[Int]",
            MethodReceiver::Map => "Map[Str, Int]",
            other => panic!("no parameter type for an `is_empty` receiver {other:?}"),
        };
        for (parameter, access) in [(ty.to_owned(), "."), (format!("{ty}?"), "?.")] {
            let source = format!(
                "proc probe(target: {parameter}) {{\n  let _ = target{access}is_empty()\n}}\n"
            );
            let diagnostics = check_and_lower(&source);
            assert!(
                diagnostics.is_empty(),
                "`is_empty` does not lower:\n{source}{diagnostics:#?}"
            );
        }
        receivers += 1;
    }
    assert_eq!(receivers, 4);
}

// The check above is only as good as its probe: a method the registry does
// not have must fail it.
#[test]
fn the_probe_rejects_a_call_that_is_not_a_method() {
    let diagnostics =
        check_and_lower("proc probe(target: Path) {\n  let _ = target.is_socket()\n}\n");
    assert!(!diagnostics.is_empty());
}
