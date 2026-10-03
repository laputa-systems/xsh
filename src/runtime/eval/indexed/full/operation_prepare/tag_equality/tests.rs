use super::*;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::{RecordMap, Value};
use crate::sema::check::Checker;

const SOURCE: &str = "enum Language { LangUnknown, LangKnown }\ntype Candidate = {language: Language, alternative: Language}\npure identity(value) { value }\npure unknown(candidate: Candidate) -> Bool { identity(candidate.language == LangUnknown) }\npure known(candidate: Candidate) -> Bool { identity(candidate.language != LangUnknown) }\npure observed(candidate: Candidate) -> Int { if candidate.language == LangUnknown { 1 } else { 0 } }\npure alternate(candidate: Candidate) -> Language { candidate.alternative }\npure other() -> Language { LangKnown }\n";

// Host fixtures dispose the checker and exercise original executable receipts.
fn fixture(source: &str) -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "original-tag-equality.xsh", crate::loader::entry_source_from_text("original-tag-equality.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let selected = bodies.solved.operations.values().filter(|operation| {
        let graph = &bodies.solved.graph;
        graph.candidate_evidence(operation.requirement).unwrap().is_some_and(|selected|
            matches!(bodies.solved.operation_catalog.candidate(graph, selected.candidate).unwrap(),
                crate::sema::check::SolvedOperationAuthority::Language(metadata) if matches!(metadata.operation,
                    PreparedLanguageOperation::Equality { op: BinaryOp::Eq | BinaryOp::Ne })))
    }).count();
    assert_eq!(selected, 3);
    parsed.arena.symbol_owner().with_current(|| {
        assert_eq!(bodies.solved.nominal_members.values().filter(|member| member.family == Name::intern("Language") && member.fields.is_empty()).count(), 2);
    });
    let weak = Arc::downgrade(&bodies.solved);
    let counters = bodies.solved.graph.counters().clone();
    let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id);
    assert_eq!(bodies.solved.graph.counters(), &counters);
    drop(parsed); drop(declarations); drop(bodies);
    assert!(weak.upgrade().is_none());
    program.unwrap()
}

#[test]
fn original_nullary_tag_equality_field_results_keep_their_nominal_family_after_frontend_disposal_on_both_routes() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(fixture(SOURCE));
        program.symbol_owner().with_current(|| {
            FullVerifier::verify(&program).unwrap();
            assert_eq!(program.generic_evidence().unwrap().operations().filter(|(_, operation)| matches!(operation.authority,
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Equality { op: BinaryOp::Eq | BinaryOp::Ne }, .. })).count(), 3);
            for recursive in [false, true] {
                for (member, unknown) in [("LangUnknown", true), ("LangKnown", false)] {
                    for (name, expected) in [("unknown", Value::Bool(unknown)), ("known", Value::Bool(!unknown)), ("observed", Value::Int(i64::from(unknown)))] {
                        let tag = Value::Tag { type_name: Name::intern("Language"), name: Arc::from(member), fields: Vec::new(), wire: None };
                        let candidate = Value::Record(RecordMap::from([(Arc::from("language"), tag.clone()), (Arc::from("alternative"), tag)]));
                        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                        evaluator.indexed_program = Some(Arc::clone(&program));
                        let function = LoweredFunctionKey::Name(Name::intern(name));
                        let mut call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure, &[candidate],
                            Span::new(program.store.source_id, 0, 0)).expect("original tag equality function exists");
                        let value = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call);
                        assert_eq!(value.unwrap(), expected);
                    }
                }
            }
        });
    });
}

fn call_unknown(program: Arc<FullProgram>, recursive: bool, tag: Value) -> Option<Result<Value, crate::runtime::value::RuntimeError>> {
    let same_family = matches!(&tag, Value::Tag { type_name, .. } if *type_name == Name::intern("Language"));
    let candidate = Value::Record(RecordMap::from([(Arc::from("language"), tag.clone()), (Arc::from("alternative"), tag)]));
    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
    evaluator.indexed_program = Some(Arc::clone(&program));
    let function = LoweredFunctionKey::Name(Name::intern("unknown"));
    let mut call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure, &[candidate], Span::new(program.store.source_id, 0, 0));
    // Foreign families fail typed argument binding before a worker starts.
    if same_family { crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call) } else { call() }
}

#[test]
fn original_nullary_tag_equality_refuses_missing_foreign_joint_operator_field_member_and_source_rewrites_on_both_routes() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture(SOURCE);
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, operation) = generic.operations().find(|(_, operation)| operation.tag_equality.as_ref().is_some_and(|recipe|
                recipe.original.op == BinaryOp::Eq && recipe.original.span.start() == SOURCE.find("candidate.language ==").unwrap())).unwrap();
            let recipe = operation.tag_equality.as_ref().unwrap();
            let instruction = recipe.original.instruction.unwrap();
            let operands = recipe.original.instructions.unwrap();
            let mut variants = Vec::new();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_operations();
            variants.push(missing);
            let foreign = fixture(SOURCE);
            let mut foreign_source = program.clone();
            foreign_source.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().source = foreign.generic_evidence().unwrap().operations().next().unwrap().1.source;
            variants.push(foreign_source);
            let mut opcode = program.clone();
            let slot = recipe.payload[0] as usize;
            opcode.store.binary_ops[slot] = BinaryOp::Ne;
            variants.push(opcode.clone());
            let proof = opcode.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap();
            proof.tag_equality.as_mut().unwrap().original.op = BinaryOp::Ne;
            proof.authority = generic.operations().find(|(_, other)| other.tag_equality.as_ref().is_some_and(|recipe| recipe.original.op == BinaryOp::Ne)).unwrap().1.authority.clone();
            let opposite = proof.authority.clone();
            let rewritten_source = opcode.store.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap();
            rewritten_source.identity = opposite.identity();
            rewritten_source.expected = opposite;
            variants.push(opcode);
            let mut field = program.clone();
            let alternate = (1..=field.store.strings.len()).find(|&index| field.store.string(index as u32).unwrap() == "alternative").unwrap() as u32;
            let range = field.store.data[operands[0] as usize].range();
            field.store.extra[range.start as usize + 1] = alternate;
            variants.push(field.clone());
            field.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().tag_equality.as_mut().unwrap().operand_payloads[0][1] = alternate;
            variants.push(field);
            let mut member = program.clone();
            let alternate = (1..=member.store.strings.len()).find(|&index| member.store.string(index as u32).unwrap() == "LangKnown").unwrap() as u32;
            let range = member.store.data[operands[1] as usize].range();
            member.store.extra[range.start as usize + 1] = alternate;
            variants.push(member.clone());
            member.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().tag_equality.as_mut().unwrap().operand_payloads[1][1] = alternate;
            let rewritten_member = member.store.generic.as_deref_mut().unwrap().test_tag_constructor_mut(operands[1]).unwrap();
            rewritten_member.original.member = Name::intern("LangKnown");
            if let QualifiedNominalIdentity::Source { member, .. } = &mut rewritten_member.original.authority { *member = Some(Name::intern("LangKnown")); }
            rewritten_member.original.application.authority = crate::sema::check::ConstructorAuthority::Nominal(rewritten_member.original.authority);
            rewritten_member.payload[1] = alternate;
            variants.push(member);
            let mut location = program.clone();
            let location_id = IrLocationId::from_raw(recipe.payload[3]).unwrap().index();
            location.store.locations[location_id].start += 1;
            variants.push(location);
            for changed in variants {
                assert!(FullVerifier::verify(&changed).is_err());
                for recursive in [false, true] {
                    let tag = Value::Tag { type_name: Name::intern("Language"), name: Arc::from("LangUnknown"), fields: Vec::new(), wire: None };
                    assert!(matches!(call_unknown(Arc::new(changed.clone()), recursive, tag), Some(Err(_))), "altered original equality instruction {instruction} executed");
                }
            }
        });
    });
}

#[test]
fn original_nullary_tag_equality_rejects_transported_foreign_members_and_payloads_on_both_routes() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(fixture(SOURCE));
        program.symbol_owner().with_current(|| {
            for recursive in [false, true] {
                for tag in [
                    Value::Tag { type_name: Name::intern("Language"), name: Arc::from("Unpublished"), fields: Vec::new(), wire: None },
                    Value::Tag { type_name: Name::intern("OtherLanguage"), name: Arc::from("LangUnknown"), fields: Vec::new(), wire: None },
                    Value::Tag { type_name: Name::intern("Language"), name: Arc::from("LangUnknown"), fields: vec![Value::Int(1)], wire: None },
                    Value::Tag { type_name: Name::intern("Language"), name: Arc::from("LangUnknown"), fields: Vec::new(),
                        wire: Some(Arc::new(crate::sema::wire_enums::WireEnumMapping { type_name: Name::intern("Language"),
                            variants: std::collections::BTreeMap::from([(Name::intern("LangUnknown"), Arc::from("unknown"))]) })) },
                ] {
                    let foreign_family = matches!(&tag, Value::Tag { type_name, .. } if *type_name != Name::intern("Language"));
                    let result = call_unknown(Arc::clone(&program), recursive, tag);
                    if foreign_family { assert!(result.is_none(), "foreign nominal family must fail the typed argument boundary"); }
                    else { assert!(matches!(result, Some(Err(_))), "accepted nominal arguments must fail the prepared equality boundary"); }
                }
            }
        });
    });
}
