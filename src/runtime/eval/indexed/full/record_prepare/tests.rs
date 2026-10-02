use super::*;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::{RecordMap, Value};
use crate::sema::check::Checker;
use crate::sema::operation_graph::PreparedLanguageOperation;

#[test]
fn original_record_spread_source_preserves_layout_after_frontend_disposal_on_both_routes() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        let source = "type Row = {text: Str}\ntype Inner = {required: Int}\ntype Nested = {payload: Inner}\ntype Counts = {blanks: Int, blobs: Map[Str, Int], code: Int, comments: Int}\npure copied(first: Str, second: Str) -> Row { let row = {...{text: first}, text: second}; row }\npure validate(raw: Any) -> Result[Row] { raw.require(Row) }\npure selected(value: Row) -> Str { let row = {...value}; row.text }\npure extra_fields(value: Row) -> Row { let row = {...value}; row }\npure nested(value: Nested) -> Int { let row = {...value}; row.payload.required }\npure stats_code() -> Int { let original: Counts = {blanks: 2, blobs: map.empty(), code: 7, comments: 3}; let row = {...original}; row.code }\n";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "record-spread.xsh", crate::loader::entry_source_from_text("record-spread.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let weak = Arc::downgrade(&checked.solved);
        let source_id = SourceMap::files(&sources).first().unwrap().id();
        let mut preparing = Evaluator::new_with_sources(Vec::new(), sources);
        preparing.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
        let program = preparing.indexed_program.take().unwrap();
        drop(preparing); drop(checked); drop(parsed);
        assert!(weak.upgrade().is_none());
        FullVerifier::verify(&program).unwrap();
        let generic = program.generic_evidence().unwrap();
        assert_eq!(generic.record_sources().count(), 5);
        for (_, proof) in generic.record_sources() {
            assert!(generic.constructor(proof.instruction).is_none(), "a spread source cannot acquire literal physical layout authority");
        }
        assert_eq!(generic.ground_projections().count(), 4);
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut execute = || {
                    assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                    let arguments = [Value::Str(Arc::from("before")), Value::Str(Arc::from("after"))];
                    let copied = evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("copied")), LoweredFunctionKind::Pure,
                        &arguments, Span::new(source_id, 0, 0)).unwrap().unwrap();
                    let raw = Value::Record(RecordMap::from([
                        (Arc::from("aardvark"), Value::Bool(true)),
                        (Arc::from("text"), Value::Str(Arc::from("after"))),
                    ]));
                    let validated = evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("validate")), LoweredFunctionKind::Pure,
                        &[raw.clone()], Span::new(source_id, 0, 0)).unwrap().unwrap();
                    let Value::Result(crate::runtime::value::ResultValue::Ok(value)) = validated else { panic!("valid record must succeed"); };
                    let selected = evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("selected")), LoweredFunctionKind::Pure,
                        &[(*value).clone()], Span::new(source_id, 0, 0)).unwrap().unwrap();
                    assert_eq!(selected, Value::Str(Arc::from("after")));
                    let extras = evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("extra_fields")), LoweredFunctionKind::Pure,
                        &[*value], Span::new(source_id, 0, 0)).unwrap().unwrap();
                    assert_eq!(extras, raw, "canonical source materialization must retain undeclared extra fields");
                    let nested = Value::Record(RecordMap::from([
                        (Arc::from("aardvark"), Value::Bool(true)),
                        (Arc::from("payload"), Value::Record(RecordMap::from([
                            (Arc::from("aardvark"), Value::Str(Arc::from("extra"))),
                            (Arc::from("required"), Value::Int(9)),
                        ]))),
                    ]));
                    let nested = evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("nested")), LoweredFunctionKind::Pure,
                        &[nested], Span::new(source_id, 0, 0)).unwrap().unwrap();
                    assert_eq!(nested, Value::Int(9), "nested construction must install each required numeric prefix");
                    let stats = evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("stats_code")), LoweredFunctionKind::Pure,
                        &[], Span::new(source_id, 0, 0)).unwrap().unwrap();
                    assert_eq!(stats, Value::Int(7), "compact record candidates must retain numeric source storage");
                    copied
                };
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute) } else { execute() };
                assert_eq!(result, Value::Record(RecordMap::from([(Arc::from("text"), Value::Str(Arc::from("after")))])));
            });
        }
    }).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

fn fixture() -> FullProgram {
    super::super::operation_prepare::tests::source_fixture(
        "pure compare(left: Str, right: Str) -> Bool { let first = {...{text: left}, suffix: right}; let second = {...{text: right}, suffix: left}; left == right }\nlet result = compare(\"yes\", \"no\")\n",
        PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn original_record_spread_source_rejects_missing_altered_and_foreign_receipts_and_operands() {
    let program = fixture();
    let foreign = fixture();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (id, source) = generic.record_sources().next().unwrap();
        let (other_id, other) = generic.record_sources().nth(1).unwrap();
        let mut missing = program.store.clone();
        missing.generic.as_deref_mut().unwrap().test_remove_record_sources();
        assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
        let mut rewritten = program.store.clone();
        let payload = rewritten.payload(rewritten.data[source.instruction as usize].range()).unwrap();
        let block = rewritten.blocks[IrBlockId::from_raw(payload[0]).unwrap().index()];
        rewritten.extra[block.instructions.start as usize + 2] = other.entries[0].instruction;
        let failure = FullVerifier::verify_generic_evidence(&rewritten).unwrap_err();
        assert!(failure.message.contains("original supplied value"), "{}", failure.message);
        rewritten.generic.as_deref_mut().unwrap().test_record_source_mut(id).unwrap().entries[0].instruction = other.entries[0].instruction;
        assert!(FullVerifier::verify_generic_evidence(&rewritten).unwrap_err().message.contains("original receipt"), "rewriting the payload and public proof together cannot replace the original spread");
        let mut changed_key = program.store.clone();
        changed_key.extra[block.instructions.start as usize + 4] = Name::intern("another").symbol().raw();
        assert!(FullVerifier::verify_generic_evidence(&changed_key).unwrap_err().message.contains("original entry kind or key"));
        let mut changed_layout = program.store.clone();
        changed_layout.generic.as_deref_mut().unwrap().test_record_layout_mut(id).unwrap().fields.reverse();
        assert!(FullVerifier::verify_generic_evidence(&changed_layout).unwrap_err().message.contains("physical schema"), "a semantically equal field set cannot substitute another numeric prefix");
        let mut changed = program.store.clone();
        changed.generic.as_deref_mut().unwrap().test_record_source_mut(id).unwrap().entries[0].instruction = other.entries[0].instruction;
        assert!(FullVerifier::verify_generic_evidence(&changed).unwrap_err().message.contains("original receipt"));
        let mut foreign_owner = program.store.clone();
        foreign_owner.generic.as_deref_mut().unwrap().test_record_source_mut(id).unwrap().owner = InstructionOwner::Driver(0);
        assert!(FullVerifier::verify_generic_evidence(&foreign_owner).is_err());
        let mut foreign_root = program.store.clone();
        foreign_root.generic.as_deref_mut().unwrap().test_record_source_mut(id).unwrap().checked = foreign.generic_evidence().unwrap().record_sources().next().unwrap().1.checked;
        assert!(FullVerifier::verify_generic_evidence(&foreign_root).unwrap_err().message.contains("original receipt"));
        let foreign_id = foreign.generic_evidence().unwrap().record_sources().next().unwrap().0;
        assert!(generic.record_source(foreign_id).is_err());
        assert!(generic.layout(foreign.generic_evidence().unwrap().record_source(foreign_id).unwrap().layout).is_err());
        assert_ne!(id, other_id);
    });
}

#[test]
fn original_record_spread_source_rejects_retired_and_replaced_receipt_handles() {
    let program = fixture();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (original_id, source) = generic.record_sources().next().unwrap();
        let mut builder = super::super::super::generic::GenericEvidenceBuilder::default();
        let mut receipt = source.clone();
        receipt.layout = builder.add_layout(generic.layout(source.layout).unwrap().clone()).unwrap();
        for (instruction, origin) in std::iter::once((source.instruction, source.origin))
            .chain(source.entries.iter().map(|entry| (entry.source_instruction, entry.origin))) {
            builder.register_instruction_origin(instruction, super::super::super::generic::OperationSourceOrigin::Expression(origin), source.owner).unwrap();
        }
        let checkpoint = builder.checkpoint();
        let retired = builder.add_record_source(receipt.clone()).unwrap();
        let stale_checkpoint = builder.checkpoint();
        builder.rewind(checkpoint).unwrap();
        let replacement = builder.add_record_source(receipt).unwrap();
        assert!(builder.rewind(stale_checkpoint).is_err());
        let owners = program.store.generic_instruction_owners().unwrap();
        let mut store = builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap();
        assert!(store.record_source(retired).is_err());
        assert!(store.record_source(original_id).is_err());
        assert_eq!(store.record_source_at(source.instruction).unwrap().unwrap().origin, source.origin);
        let restored = store.record_source(replacement).unwrap();
        assert_eq!(restored.entries.len(), source.entries.len());
        for (restored, original) in restored.entries.iter().zip(source.entries.iter()) {
            assert_eq!((restored.kind, restored.origin, restored.instruction, restored.source_instruction, restored.ty),
                (original.kind, original.origin, original.instruction, original.source_instruction, original.ty));
        }
        let bytes = store.retained_bytes();
        store.shrink_to_fit();
        assert!(store.retained_bytes() <= bytes);
        assert!(store.record_source(replacement).is_ok());
    });
}
