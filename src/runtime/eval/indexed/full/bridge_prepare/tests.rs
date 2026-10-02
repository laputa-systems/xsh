use super::*;
use crate::runtime::eval::{Evaluator, LoweredFunctionKey, LoweredFunctionKind};
use crate::runtime::value::{ResultValue, Value};
use crate::sema::check::Checker;
use crate::symbol::QualifiedName;

const SOURCE: &str = "";

fn catalog_fixture() -> (SourceMap, crate::syntax::arena::ArenaProgram) {
    let mut sources = SourceMap::new();
    let entry = sources.add_file("original-native-bridge.xsh", SOURCE);
    let catalog = crate::stdlib::find("json").unwrap();
    let module = sources.add_file(catalog.label, catalog.source);
    let mut builder = crate::syntax::arena::ArenaProgramBuilder::with_token_capacity(1024);
    let parsed = crate::syntax::parser::Parser::parse_source_into_arena_builder(entry, SOURCE, &mut builder);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let diagnostics = builder.parse_catalog_arena_module(catalog, module);
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    (sources, builder.finish_with_statements(parsed.statements))
}

fn fixture() -> FullProgram {
    let (sources, parsed) = catalog_fixture();
    let source_id = sources.files().first().unwrap().id();
    let declarations = Checker::check_compact_declarations(&parsed);
    let bodies = Checker::probe_compact_bodies(&parsed, &declarations);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let owner = Arc::downgrade(&declarations.solved);
    let counters = declarations.solved.graph.counters().clone();
    let program = FullBuilder::build_compact(&parsed, &declarations, &bodies, SOURCE, Arc::new(sources), source_id).unwrap();
    assert_eq!(&counters, declarations.solved.graph.counters());
    drop(parsed); drop(declarations); drop(bodies);
    assert!(owner.upgrade().is_none());
    FullVerifier::verify(&program).unwrap();
    program
}

#[test]
fn original_native_bridge_keeps_exact_catalog_signature_and_operand_after_frontend_disposal() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let sources: Vec<_> = generic.bridge_calls().collect();
            let [source] = sources.as_slice() else { panic!("the catalog has one original type-name invocation"); };
            assert_eq!(source.original.declaration().op(), RuntimeOp::BridgeTypeName);
            assert_eq!(source.original.declaration().function(), "type_name");
            assert_eq!(program.store.semantic.to_type(source.formal).unwrap(), Type::Any);
            assert_eq!(program.store.semantic.to_type(source.actual).unwrap(), Type::List(Box::new(Type::Any)));
            assert_eq!(FullVerifier::bridge_result(&program.store, generic, source.instruction, source.owner).unwrap(), Type::Str);
            assert!(!program.store.functions.iter().any(|function| program.store.string(function.name).unwrap() == "type_name"), "native linkage never emits a placeholder function body");
        });
    });
}

#[test]
fn original_native_bridge_refuses_missing_foreign_replaced_and_changed_operand_receipts() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        let foreign = fixture();
        program.symbol_owner().with_current(|| {
            let source = program.generic_evidence().unwrap().bridge_calls().next().unwrap();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_bridge_calls();
            assert!(FullVerifier::verify(&missing).is_err());
            let mut altered = program.clone();
            let block = altered.store.blocks[IrBlockId::from_raw(source.argument_block.0).unwrap().index()];
            altered.store.extra[block.instructions.start as usize + 2] = source.instruction;
            assert!(FullVerifier::verify(&altered).is_err(), "changed physical operand cannot replace its original recipe");
            let mut coforged = altered.clone();
            let changed = coforged.store.generic.as_deref_mut().unwrap().test_bridge_call_mut(source.instruction);
            changed.operand = source.instruction;
            changed.argument_block.1[2] = source.instruction;
            assert!(FullVerifier::verify(&coforged).is_err(), "coforged semantic and physical copies cannot replace the protected receipt");
            let mut substituted = program.clone();
            substituted.store.generic.as_deref_mut().unwrap().test_replace_bridge_calls(foreign.generic_evidence().unwrap());
            assert!(FullVerifier::verify(&substituted).is_err(), "foreign prepared program cannot supply a catalog invocation receipt");
            for formal in [true, false] {
                let mut changed = program.clone();
                let receipt = changed.store.generic.as_deref_mut().unwrap().test_bridge_call_mut(source.instruction);
                if formal { receipt.formal = source.result; } else { receipt.result = source.formal; }
                assert!(FullVerifier::verify(&changed).is_err(), "a same-owned consumer cannot rewrite the native signature");
            }
        });
    });
}

#[test]
fn original_native_bridge_refuses_changed_original_call_and_declaration_facts_before_publication() {
    crate::runtime::eval::run_eval(|| {
        let (sources, parsed) = catalog_fixture();
        let source_id = sources.files().first().unwrap().id();
        for control in 0..5 {
            let mut declarations = Checker::check_compact_declarations(&parsed);
            let solved = Arc::get_mut(&mut declarations.solved).unwrap();
            let (&origin, call) = solved.calls.iter().find(|(_, call)| call.declaration.is_some_and(|identity| solved.embedded_bridge(identity).is_some())).unwrap();
            let declaration = call.declaration.unwrap();
            let caller = call.caller.unwrap();
            match control {
                0 => { solved.embedded_bridges.remove(&declaration); }
                1 => { solved.calls.get_mut(&origin).unwrap().caller = Some(declaration); }
                2 => { solved.calls.get_mut(&origin).unwrap().binding.supplied_slots.clear(); }
                3 => { solved.expression_owners.insert(origin, declaration); }
                _ => { let signature = solved.declarations[&caller].signature; solved.declarations.get_mut(&declaration).unwrap().signature = signature; }
            }
            let bodies = Checker::probe_compact_bodies(&parsed, &declarations);
            assert!(FullBuilder::build_compact(&parsed, &declarations, &bodies, SOURCE, Arc::new(sources.clone()), source_id).is_err(), "changed original bridge authority cannot be published");
        }
    });
}

#[test]
fn original_native_bridge_executes_declared_list_operand_on_both_indexed_routes() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(fixture());
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = LoweredFunctionKey::Qualified(QualifiedName { namespace: Name::intern(crate::stdlib::namespace_text("json")), member: Name::intern("encode_lines") });
                assert!(evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[Value::Int(7)], Span::at(program.store.source_id, 0)).is_none(), "the host binder preserves the declared List[Any] parameter");
                let arguments = [Value::List(vec![Value::Int(7)])];
                let execute = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::at(program.store.source_id, 0)).unwrap();
                let value = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, execute).unwrap();
                let Value::Result(ResultValue::Ok(encoded)) = value else { panic!("the module returns its declared successful Result carrier"); };
                assert_eq!(*encoded, Value::Str(Arc::from("7\n")));
            });
        }
    });
}
