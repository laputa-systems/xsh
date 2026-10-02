use super::*;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::{RecordMap, Value};
use crate::sema::check::Checker;
use crate::sema::operation_graph::PreparedLanguageOperation;

#[test]
fn original_record_constructor_preserves_spread_defaults_and_numeric_layout_after_frontend_disposal() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        let source = "type Pair[T] = {left: T, right: T, items: List[T] = []}\ntype Concrete = Pair[UInt]\npure make(left: UInt, right: UInt) -> Concrete { Concrete(...{left: left}, right: right) }\npure selected(left: UInt, right: UInt) -> UInt { let row = make(left, right); row.right }\n";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("record-constructor.xsh", crate::loader::entry_source_from_text("record-constructor.xsh", source.to_owned()), Vec::new());
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
        assert_eq!(program.generic_evidence().unwrap().record_constructors().count(), 1);
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut execute = || {
                    let row = evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("make")), LoweredFunctionKind::Pure,
                        &[Value::Int(7), Value::Int(8)], Span::new(source_id, 0, 0)).unwrap().unwrap();
                    let Value::Record(RecordMap::Shaped { .. }) = &row else { panic!("validated constructor preserves its prepared physical schema"); };
                    assert_eq!(row, Value::Record(RecordMap::from([
                        (Arc::from("left"), Value::Int(7)), (Arc::from("right"), Value::Int(8)), (Arc::from("items"), Value::List(Vec::new())),
                    ])));
                    let selected = evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern("selected")), LoweredFunctionKind::Pure,
                        &[Value::Int(7), Value::Int(8)], Span::new(source_id, 0, 0)).unwrap().unwrap();
                    assert_eq!(selected, Value::Int(8));
                };
                if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute); } else { execute(); }
            });
        }
    }).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

fn fixture() -> FullProgram {
    super::super::operation_prepare::tests::source_fixture(
        "type Row = {left: Str, right: Str, count: UInt = 3}\npure compare(left: Str, right: Str) -> Bool { let first = Row(right: right, ...{left: left}); let second = Row(left: right, right: left); left == right }\nlet result = compare(\"yes\", \"no\")\n",
        PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn original_record_constructor_rejects_changed_operands_defaults_schema_and_public_receipts() {
    let program = fixture();
    let foreign = fixture();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (id, source) = generic.record_constructors().next().unwrap();
        let other = generic.record_constructors().nth(1).unwrap().1;
        let mut missing = program.store.clone();
        missing.generic.as_deref_mut().unwrap().test_remove_record_constructors();
        assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
        let record_payload = program.store.payload(program.store.data[source.record as usize].range()).unwrap();
        let block = program.store.blocks[IrBlockId::from_raw(record_payload[0]).unwrap().index()];
        let mut substituted = program.store.clone();
        substituted.extra[block.instructions.start as usize + 3] = other.fields[0];
        assert!(FullVerifier::verify_generic_evidence(&substituted).is_err());
        substituted.generic.as_deref_mut().unwrap().test_record_constructor_mut(id).unwrap().fields[0] = other.fields[0];
        assert!(FullVerifier::verify_generic_evidence(&substituted).unwrap_err().message.contains("original receipt"));
        let default_slot = source.original.defaults[0].0;
        let default = source.fields[default_slot];
        let mut changed_default = program.store.clone();
        changed_default.extra[changed_default.data[default as usize].lhs as usize] = 9;
        assert!(FullVerifier::verify_generic_evidence(&changed_default).is_err());
        let mut changed_slot = program.store.clone();
        changed_slot.generic.as_deref_mut().unwrap().test_record_constructor_mut(id).unwrap().original.application.supplied[0].slot = default_slot;
        assert!(FullVerifier::verify_generic_evidence(&changed_slot).unwrap_err().message.contains("original receipt"));
        let mut changed_application = program.store.clone();
        changed_application.generic.as_deref_mut().unwrap().test_record_constructor_mut(id).unwrap().original.checked = foreign.generic_evidence().unwrap().record_constructors().next().unwrap().1.original.checked;
        assert!(FullVerifier::verify_generic_evidence(&changed_application).unwrap_err().message.contains("original receipt"));
        let mut changed_schema = program.store.clone();
        let row = source.rows.iter().find(|row| row.schema.is_some()).unwrap();
        let schema = &mut changed_schema.prepared_schemas[row.payload[4] as usize];
        let crate::runtime::eval::require::PreparedSchema::Record(fields) = Arc::make_mut(schema) else { unreachable!() };
        fields.reverse();
        assert!(FullVerifier::verify_generic_evidence(&changed_schema).is_err());
        assert!(generic.record_constructor(foreign.generic_evidence().unwrap().record_constructors().next().unwrap().0).is_err());
    });
}

#[test]
fn original_record_constructor_rejects_rewound_handles_and_accounts_compaction() {
    let program = fixture();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (foreign_id, source) = generic.record_constructors().next().unwrap();
        let mut builder = super::super::super::generic::GenericEvidenceBuilder::default();
        builder.register_instruction_origin(source.instruction, super::super::super::generic::OperationSourceOrigin::Expression(source.original.origin), source.owner).unwrap();
        let checkpoint = builder.checkpoint();
        let retired = builder.add_record_constructor(source.clone()).unwrap();
        let stale_checkpoint = builder.checkpoint();
        builder.rewind(checkpoint).unwrap();
        let replacement = builder.add_record_constructor(source.clone()).unwrap();
        assert!(builder.rewind(stale_checkpoint).is_err());
        let owners = program.store.generic_instruction_owners().unwrap();
        let mut evidence = builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap();
        assert!(evidence.record_constructor(retired).is_err());
        assert!(evidence.record_constructor(foreign_id).is_err());
        let bytes = evidence.retained_bytes();
        evidence.shrink_to_fit();
        assert!(evidence.retained_bytes() <= bytes);
        assert_eq!(evidence.record_constructor(replacement).unwrap().original.origin, source.original.origin);
    });
}

#[test]
fn original_record_constructor_rejects_same_bundle_phantom_application_substitution() {
    let program = super::super::operation_prepare::tests::source_fixture(
        "type Marker[T] = {label: Str}\ntype Word = Marker[Str]\ntype Count = Marker[UInt]\npure compare(left: Str, right: Str) -> Bool { let first = Word(label: left); let second = Count(label: right); left == right }\nlet result = compare(\"yes\", \"no\")\n",
        PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (id, first) = generic.record_constructors().next().unwrap();
        let second = generic.record_constructors().nth(1).unwrap().1;
        assert_eq!(first.result, second.result, "phantom arguments do not change the physical field row");
        let mut changed = program.store.clone();
        changed.generic.as_deref_mut().unwrap().test_record_constructor_mut(id).unwrap().original.application.authority = second.original.application.authority.clone();
        assert!(FullVerifier::verify_generic_evidence(&changed).unwrap_err().message.contains("original receipt"));
    });
}

fn nominal_scope_fixture() -> Arc<FullProgram> {
    let source = "enum Mode: Str { On = \"on\", Off = \"off\" }\ntype Pair[T] = {marker: T, backup: T}\ntype Packet[T] = {pair: Pair[T], count: UInt = 3}\ntype PairMode = Pair[Mode]\ntype PacketMode = Packet[Mode]\npure make(mode: Mode, token) -> PacketMode { PacketMode(pair: PairMode(marker: mode, backup: mode)) }\npure fixed(mode: Mode) -> PacketMode { PacketMode(pair: PairMode(marker: mode, backup: mode)) }\npure published() -> PacketMode { make(On, \"ignored\") }\npure updated() -> PacketMode { let row = make(Off, 0); {...row, pair.backup: On, count: 4} }\npure selected() -> Mode { updated().pair.marker }\npure other_selected() -> Mode { updated().pair.backup }\npure alternate() -> Mode { let ignored = published().pair.backup; updated().pair.marker }\n";
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("nominal-constructor-scope.xsh", crate::loader::entry_source_from_text("nominal-constructor-scope.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let weak = Arc::downgrade(&checked.solved);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
    let program = evaluator.indexed_program.take().unwrap();
    drop(evaluator); drop(parsed); drop(checked);
    assert!(weak.upgrade().is_none(), "the constructor's scoped authority must survive frontend disposal independently");
    program.symbol_owner().with_current(|| FullVerifier::verify(&program).unwrap());
    program
}

#[test]
fn original_record_constructor_preserves_ground_nominal_fields_in_a_quantified_owner_on_both_routes_after_frontend_disposal() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        let program = nominal_scope_fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (_, scope) = generic.scopes().find(|(_, scope)| !scope.quantifiers.is_empty()).expect("the unrelated token retains its enclosing quantifier");
            assert!(generic.record_constructors().any(|(_, constructor)| constructor.owner == InstructionOwner::Function(scope.owner)
                && constructor.original.checked.scope.is_some()), "the ground constructor keeps its enclosing lexical root");
            assert!(generic.record_update_sources().count() >= 1);
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut execute = || {
                    let mut values = Vec::new();
                    for function in ["published", "updated", "selected", "other_selected"] {
                        values.push(evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern(function)), LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0)).unwrap().unwrap());
                    }
                    for (row, count, marker, backup) in [(&values[0], 3, "On", "On"), (&values[1], 4, "Off", "On")] {
                        let Value::Record(RecordMap::Shaped { .. }) = row else { panic!("nominal record constructor preserves canonical row slots"); };
                        let Value::Record(fields) = row else { unreachable!() };
                        assert_eq!(fields.get("count"), Some(&Value::Int(count)));
                        let Some(Value::Record(pair)) = fields.get("pair") else { panic!("constructor preserves its nested record"); };
                        assert!(matches!(pair, RecordMap::Shaped { .. }));
                        for (field, expected) in [("marker", marker), ("backup", backup)] {
                            let Some(Value::Tag { type_name, name, fields, wire }) = pair.get(field) else { panic!("constructor's nominal field stays tagged"); };
                            assert_eq!(*type_name, Name::intern("Mode"));
                            assert_eq!(name.as_ref(), expected);
                            assert!(fields.is_empty());
                            assert!(Arc::ptr_eq(wire.as_ref().unwrap(), &program.store.wire_enums[0]));
                        }
                    }
                    for (value, expected) in [(&values[2], "Off"), (&values[3], "On")] {
                        let Value::Tag { type_name, name, fields, wire } = value else { panic!("numeric projection preserves the original nominal carrier"); };
                        assert_eq!(*type_name, Name::intern("Mode"));
                        assert_eq!(name.as_ref(), expected);
                        assert!(fields.is_empty());
                        assert!(Arc::ptr_eq(wire.as_ref().unwrap(), &program.store.wire_enums[0]));
                    }
                };
                if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute); } else { execute(); }
            }
        });
    }).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

#[test]
fn original_record_constructor_rejects_equal_row_scope_and_nominal_projection_substitution() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        let program = nominal_scope_fixture();
        let foreign = nominal_scope_fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (_, scope) = generic.scopes().find(|(_, scope)| !scope.quantifiers.is_empty()).unwrap();
            let (id, source) = generic.record_constructors().find(|(_, source)| source.owner == InstructionOwner::Function(scope.owner)).unwrap();
            let fixed = generic.record_constructors().find(|(_, candidate)| candidate.owner != source.owner && candidate.result == source.result).unwrap().1;
            assert_ne!(source.original.checked.scope, fixed.original.checked.scope, "equal record rows still belong to distinct lexical schemes");
            let mut changed_scope = program.store.clone();
            changed_scope.generic.as_deref_mut().unwrap().test_record_constructor_mut(id).unwrap().original.checked.scope = fixed.original.checked.scope;
            assert!(FullVerifier::verify_generic_evidence(&changed_scope).unwrap_err().message.contains("original receipt"));
            let mut foreign_root = program.store.clone();
            foreign_root.generic.as_deref_mut().unwrap().test_record_constructor_mut(id).unwrap().original.checked = foreign.generic_evidence().unwrap().record_constructors().next().unwrap().1.original.checked;
            assert!(FullVerifier::verify_generic_evidence(&foreign_root).is_err());
            let projection = generic.ground_projections().find_map(|(_, projection)| {
                let source = generic.ground_projection_source(projection.source).unwrap();
                (source.field == Name::intern("marker")).then_some(source)
            }).unwrap();
            let backup = generic.ground_projections().find_map(|(_, candidate)| {
                let source = generic.ground_projection_source(candidate.source).unwrap();
                (source.field == Name::intern("backup") && source.result == projection.result).then_some(source)
            }).unwrap();
            let payload = program.store.data[projection.instruction as usize].range().bounds(program.store.extra.len()).unwrap();
            let backup_words = program.store.payload(program.store.data[backup.instruction as usize].range()).unwrap();
            let mut changed_field = program.store.clone();
            changed_field.extra[payload.start + 1] = backup_words[1];
            assert!(FullVerifier::verify_generic_evidence(&changed_field).is_err());
            let (wrapped, alternate) = generic.ground_projections().find_map(|(_, proof)| {
                let source = generic.ground_projection_source(proof.source).unwrap();
                if source.receiver_wrappers.is_empty() { return None; }
                generic.ground_projections().find_map(|(_, other)| {
                    let other = generic.ground_projection_source(other.source).unwrap();
                    (other.owner == source.owner && other.receiver == source.receiver && other.receiver_source_instruction != source.receiver_source_instruction).then_some((source, other))
                })
            }).expect("two same-typed call receivers retain independent wrapper lineages");
            let wrapper = &wrapped.receiver_wrappers[0];
            let wrapper_words = program.store.data[wrapper.instruction as usize].range().bounds(program.store.extra.len()).unwrap();
            let mut changed_wrapper = program.store.clone();
            changed_wrapper.extra[wrapper_words.start] = alternate.receiver_source_instruction;
            assert!(FullVerifier::verify_generic_evidence(&changed_wrapper).is_err());
            let mut missing_nominal = program.store.clone();
            missing_nominal.wire_enums.clear();
            assert!(FullVerifier::verify_generic_evidence(&missing_nominal).is_err());
        });
    }).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}
