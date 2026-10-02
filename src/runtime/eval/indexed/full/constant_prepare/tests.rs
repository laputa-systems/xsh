use super::*;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::{RecordMap, Value};
use crate::sema::check::Checker;
use crate::sema::operation_graph::PreparedLanguageOperation;

#[test]
fn original_prepared_constant_preserves_nested_layout_uint_and_nominal_after_frontend_disposal() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        let source = "type Payload = {aardvark: Bool, required: UInt}\ntype Data = {aardvark: Str, payload: Payload, required: UInt}\nenum Mode: Str { On = \"on\", Off = \"off\" }\nconst data: Data = {aardvark: \"extra\", payload: {aardvark: true, required: 7}, required: 9}\nconst mode: Mode = On\npure published() -> Data { data }\npure selected() -> Int { data.payload.required + data.required }\npure kept() -> Mode { mode }\n";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("constant-source.xsh", crate::loader::entry_source_from_text("constant-source.xsh", source.to_owned()), Vec::new());
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
        assert!(program.generic_evidence().unwrap().constant_sources().count() >= 3);
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut execute = || {
                    let row = evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("published")), LoweredFunctionKind::Pure, &[], Span::new(source_id, 0, 0)).unwrap().unwrap();
                    let Value::Record(RecordMap::Shaped { .. }) = &row else { panic!("constant publication must retain the original numeric layout"); };
                    assert_eq!(row, Value::Record(RecordMap::from([
                        (Arc::from("aardvark"), Value::Str(Arc::from("extra"))), (Arc::from("required"), Value::Int(9)),
                        (Arc::from("payload"), Value::Record(RecordMap::from([(Arc::from("aardvark"), Value::Bool(true)), (Arc::from("required"), Value::Int(7))]))),
                    ])));
                    let selected = evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("selected")), LoweredFunctionKind::Pure, &[], Span::new(source_id, 0, 0)).unwrap().unwrap();
                    assert_eq!(selected, Value::Int(16));
                    let mode = evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("kept")), LoweredFunctionKind::Pure, &[], Span::new(source_id, 0, 0)).unwrap().unwrap();
                    let Value::Tag { type_name, name, fields, wire } = mode else { panic!("original nominal constant remains nominal"); };
                    assert_eq!(type_name, Name::intern("Mode"));
                    assert_eq!(name.as_ref(), "On");
                    assert!(fields.is_empty());
                    assert_eq!(wire.as_ref().unwrap().type_name, type_name);
                };
                if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute); } else { execute(); }
            });
        }
    }).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

fn fixture() -> FullProgram {
    super::super::operation_prepare::tests::source_fixture(
        "type Row = {required: UInt, text: Str}\nconst first: Row = {required: 3, text: \"left\"}\nconst second: Row = {required: 4, text: \"right\"}\npure compare(left: Str, right: Str) -> Bool { let a = first; let b = second; left == right }\nlet result = compare(\"yes\", \"no\")\n",
        PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn original_prepared_constant_rejects_pool_replacement_missing_foreign_and_coforged_receipts() {
    let program = fixture();
    let foreign = fixture();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (id, source) = generic.constant_sources().next().unwrap();
        let second = generic.constant_sources().find(|(_, candidate)| candidate.pool != source.pool && candidate.ty == source.ty).unwrap().1;
        let mut missing = program.store.clone();
        missing.generic.as_deref_mut().unwrap().test_remove_constant_sources();
        assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
        let mut changed = program.store.clone();
        changed.prepared_constants[source.pool as usize].0 = second.value.clone();
        assert!(FullVerifier::verify_generic_evidence(&changed).unwrap_err().message.contains("prepared constant changes"));
        changed.generic.as_deref_mut().unwrap().test_constant_source_mut(id).unwrap().value = second.value.clone();
        assert!(FullVerifier::verify_generic_evidence(&changed).unwrap_err().message.contains("original receipt"));
        let mut reordered = program.store.clone();
        let LoweredValue::RecordVec(fields) = &mut reordered.prepared_constants[source.pool as usize].0 else { panic!("constant has canonical physical storage"); };
        Arc::make_mut(fields).reverse();
        assert!(FullVerifier::verify_generic_evidence(&reordered).is_err());
        let mut selected = program.store.clone();
        selected.generic.as_deref_mut().unwrap().test_constant_source_mut(id).unwrap().original.selection = second.original.selection;
        assert!(FullVerifier::verify_generic_evidence(&selected).unwrap_err().message.contains("original receipt"));
        let mut wrong_root = program.store.clone();
        wrong_root.generic.as_deref_mut().unwrap().test_constant_source_mut(id).unwrap().original.checked = foreign.generic_evidence().unwrap().constant_sources().next().unwrap().1.original.checked;
        assert!(FullVerifier::verify_generic_evidence(&wrong_root).unwrap_err().message.contains("original receipt"));
        assert!(generic.constant_source(foreign.generic_evidence().unwrap().constant_sources().next().unwrap().0).is_err());
    });
}

#[test]
fn original_prepared_constant_rejects_retired_receipts_and_preserves_heap_accounting_after_compaction() {
    let program = fixture();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (foreign_id, source) = generic.constant_sources().next().unwrap();
        let mut builder = super::super::super::generic::GenericEvidenceBuilder::default();
        builder.register_instruction_origin(source.instruction, super::super::super::generic::OperationSourceOrigin::Expression(source.original.origin), source.owner).unwrap();
        let checkpoint = builder.checkpoint();
        let retired = builder.add_constant_source(source.clone()).unwrap();
        let stale_checkpoint = builder.checkpoint();
        builder.rewind(checkpoint).unwrap();
        let current = builder.add_constant_source(source.clone()).unwrap();
        assert!(builder.rewind(stale_checkpoint).is_err());
        let owners = program.store.generic_instruction_owners().unwrap();
        let mut evidence = builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap();
        assert!(evidence.constant_source(retired).is_err());
        assert!(evidence.constant_source(foreign_id).is_err());
        let bytes = evidence.retained_bytes();
        evidence.shrink_to_fit();
        assert!(evidence.retained_bytes() <= bytes);
        assert_eq!(evidence.constant_source(current).unwrap().original.selection, source.original.selection);
    });
}
