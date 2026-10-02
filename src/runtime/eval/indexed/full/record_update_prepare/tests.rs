use super::*;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::{RecordMap, Value};
use crate::sema::check::Checker;
use crate::sema::operation_graph::PreparedLanguageOperation;

#[test]
fn original_record_update_preserves_extra_fields_and_nested_numeric_layout_after_frontend_disposal_on_both_routes() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        let source = "type Nested = {required: Int, text: Str}\ntype Row = {nested: Nested, required: Int}\npure relay(row: Row) -> Row { row }\npure overlay(base: Row) -> Row { {...relay(row: base), nested.required: 2, required: 3} }\npure selected(base: Row) -> Int { {...base, nested.required: 2, required: 3}.nested.required }\n";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("record-update-source.xsh", crate::loader::entry_source_from_text("record-update-source.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let weak = Arc::downgrade(&checked.solved);
        let source_id = SourceMap::files(&sources).first().unwrap().id();
        let mut preparing = Evaluator::new_with_sources(Vec::new(), sources);
        preparing.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
        let program = preparing.indexed_program.take().unwrap();
        drop(preparing); drop(parsed); drop(checked);
        assert!(weak.upgrade().is_none());
        FullVerifier::verify(&program).unwrap();
        assert_eq!(program.generic_evidence().unwrap().record_update_sources().count(), 2);
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let base = Value::Record(RecordMap::from([
                    (Arc::from("aardvark"), Value::Str(Arc::from("outer-extra"))), (Arc::from("required"), Value::Int(1)),
                    (Arc::from("nested"), Value::Record(RecordMap::from([
                        (Arc::from("aardvark"), Value::Bool(true)), (Arc::from("required"), Value::Int(0)), (Arc::from("text"), Value::Str(Arc::from("untouched"))),
                    ]))),
                ]));
                let mut execute = || {
                    let row = evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("overlay")), LoweredFunctionKind::Pure, std::slice::from_ref(&base), Span::new(source_id, 0, 0)).unwrap().unwrap();
                    let Value::Record(RecordMap::Shaped { .. }) = &row else { panic!("record update publication retains its canonical numeric layout"); };
                    assert_eq!(row, Value::Record(RecordMap::from([
                        (Arc::from("aardvark"), Value::Str(Arc::from("outer-extra"))), (Arc::from("required"), Value::Int(3)),
                        (Arc::from("nested"), Value::Record(RecordMap::from([
                            (Arc::from("aardvark"), Value::Bool(true)), (Arc::from("required"), Value::Int(2)), (Arc::from("text"), Value::Str(Arc::from("untouched"))),
                        ]))),
                    ])));
                    let selected = evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("selected")), LoweredFunctionKind::Pure, std::slice::from_ref(&base), Span::new(source_id, 0, 0)).unwrap().unwrap();
                    assert_eq!(selected, Value::Int(2));
                };
                if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute); } else { execute(); }
            });
        }
    }).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

fn fixture() -> FullProgram {
    super::super::operation_prepare::tests::source_fixture(
        "pure value(left: Int, right: Int) -> Int { let base = {a: {b: 1, c: 0}}; let updated = {...base, a.b: left, a.c: right}; updated.a.b - updated.a.c }\nlet result = value(2, 3)\n",
        PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: crate::sema::operation_graph::ArithmeticDomain::Integer { left: crate::sema::inference::Atom::Int, right: crate::sema::inference::Atom::Int } })
}

#[test]
fn original_record_update_rejects_same_carrier_paths_replacements_missing_foreign_and_coforged_sources() {
    let program = fixture();
    let foreign = fixture();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (id, source) = generic.record_update_sources().next().unwrap();
        let payload = program.store.data[source.instruction as usize].range().bounds(program.store.extra.len()).unwrap();
        let block = IrBlockId::from_raw(program.store.extra[payload.start + 1]).unwrap();
        let entries = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
        let path = IrBlockId::from_raw(program.store.extra[entries.start + 1]).unwrap();
        let names = program.store.blocks[path.index()].instructions.bounds(program.store.extra.len()).unwrap();
        let mut missing = program.store.clone();
        missing.generic.as_deref_mut().unwrap().test_remove_record_update_sources();
        assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
        let mut replaced = program.store.clone();
        replaced.extra[entries.start + 2] = source.replacements[1].value.instruction;
        assert!(FullVerifier::verify_generic_evidence(&replaced).unwrap_err().message.contains("original supplied replacement"));
        replaced.generic.as_deref_mut().unwrap().test_record_update_source_mut(id).unwrap().replacements[0].value.instruction = source.replacements[1].value.instruction;
        assert!(FullVerifier::verify_generic_evidence(&replaced).unwrap_err().message.contains("original receipt"));
        let mut changed_path = program.store.clone();
        changed_path.extra[names.start + 2] = Name::intern("c").symbol().raw();
        assert!(FullVerifier::verify_generic_evidence(&changed_path).is_err());
        let mut changed_leaf = program.store.clone();
        let value = &source.replacements[0].value;
        let value_words = changed_leaf.data[value.source_instruction as usize].range().bounds(changed_leaf.extra.len()).unwrap();
        let second = &source.replacements[1].value;
        changed_leaf.extra[value_words.start] = second.source_payload[0];
        assert!(FullVerifier::verify_generic_evidence(&changed_leaf).is_err());
        let mut wrong_root = program.store.clone();
        wrong_root.generic.as_deref_mut().unwrap().test_record_update_source_mut(id).unwrap().checked = foreign.generic_evidence().unwrap().record_update_sources().next().unwrap().1.checked;
        assert!(FullVerifier::verify_generic_evidence(&wrong_root).is_err());
        assert!(generic.record_update_source(foreign.generic_evidence().unwrap().record_update_sources().next().unwrap().0).is_err());
    });
}

#[test]
fn original_record_update_rejects_retired_sources_and_preserves_accounting_after_compaction() {
    let program = fixture();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (foreign_id, source) = generic.record_update_sources().next().unwrap();
        let mut builder = super::super::super::generic::GenericEvidenceBuilder::default();
        let mut source = source.clone();
        let layout = generic.layout(source.layout).unwrap().clone();
        source.layout = builder.add_layout(layout).unwrap();
        builder.register_instruction_origin(source.instruction, super::super::super::generic::OperationSourceOrigin::Expression(source.origin), source.owner).unwrap();
        for value in std::iter::once(&source.base).chain(source.replacements.iter().map(|replacement| &replacement.value)) {
            builder.register_instruction_origin(value.source_instruction, super::super::super::generic::OperationSourceOrigin::Expression(value.origin), source.owner).unwrap();
        }
        let checkpoint = builder.checkpoint();
        let retired = builder.add_record_update_source(source.clone()).unwrap();
        let stale_checkpoint = builder.checkpoint();
        builder.rewind(checkpoint).unwrap();
        let current = builder.add_record_update_source(source).unwrap();
        assert!(builder.rewind(stale_checkpoint).is_err());
        let owners = program.store.generic_instruction_owners().unwrap();
        let mut evidence = builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap();
        assert!(evidence.record_update_source(retired).is_err());
        assert!(evidence.record_update_source(foreign_id).is_err());
        let bytes = evidence.retained_bytes();
        evidence.shrink_to_fit();
        assert!(evidence.retained_bytes() <= bytes);
        assert_eq!(evidence.record_update_source(current).unwrap().replacements.len(), 2);
    });
}
