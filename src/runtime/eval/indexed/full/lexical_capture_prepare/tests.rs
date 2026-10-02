use super::*;
use crate::runtime::eval::{Evaluator, run_eval};
use crate::sema::check::Checker;
use crate::syntax::parser::Parser;

fn prepare(source: &str) -> (Evaluator, crate::runtime::eval::CompactIndexedRunPlan) {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("lexical-capture.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let symbols = parsed.arena.symbol_owner().clone();
    symbols.with_current(|| {
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let solved = Arc::downgrade(&checked.solved);
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
        let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
        drop(checked); drop(parsed);
        assert!(solved.upgrade().is_none(), "the prepared capture retains no checker owner");
        (evaluator, plan)
    })
}

#[test]
fn original_lexical_capture_executes_pure_and_proc_values_after_frontend_drop_on_both_routes() {
    run_eval(|| {
        for source in [
            "let base: Int = 3\npure plus(value: Int) -> Int { value + base }\nlet alias = plus\nprint ${alias(4)}\n",
            "let base: Int = 3\nproc plus(value: Int) [] -> Int { value + base }\nlet alias = plus\nprint ${alias(4)}\n",
        ] {
            for recursive in [false, true] {
                let (evaluator, plan) = prepare(source);
                let program = evaluator.indexed_program.as_ref().unwrap();
                let symbols = program.symbol_owner().clone();
                let generic = program.generic_evidence().unwrap();
                let (_, read) = generic.lexical_capture_sources().next().unwrap();
                let capture = generic.lexical_capture(read.capture).unwrap();
                assert_eq!(capture.binding, read.binding);
                assert_eq!(capture.definition_owner, None);
                assert_eq!(program.store.semantic.to_type(capture.ty).unwrap(), Type::Int);
                let output = run_eval(move || symbols.with_current(|| {
                    crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("plus")), recursive, || {
                        evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("the original lexical environment remains installed"))
                    })
                }));
                assert_eq!(output.status, 0, "{:?}", output.diagnostics);
                assert_eq!(output.stdout, b"7\n");
                assert!(output.stderr.is_empty());
            }
        }
    });
}

#[test]
fn original_lexical_capture_typed_ports_keep_original_tag_and_binding_after_frontend_drop() {
    run_eval(|| {
        let (evaluator, _) = prepare("var observed = 0\nvar unrelated = 9\nvar enabled = false\npure number(value: Int) -> Int { let answer: Int = value + observed; let ready: Bool = enabled and true; if ready { answer } else { 0 } }\nproc write() [] -> Unit { observed = 4 }\n");
        let program = evaluator.indexed_program.as_ref().unwrap();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            for tag in [FullTag::IntSlot, FullTag::BoolSlot] {
                let (id, source) = generic.lexical_capture_sources().find(|(_, source)| source.tag == tag).expect("the original typed capture read retains its own physical port");
                let source = source.clone();
                let allocation = generic.lexical_capture(source.capture).unwrap().clone();
                let mut changed_tag = (**program).clone();
                changed_tag.store.tags[source.instruction as usize] = FullTag::ExprParam;
                assert!(FullVerifier::verify_lexical_capture_source(&changed_tag.store, generic, &source).is_err(), "a compatible storage shape cannot replace the original typed port");
                let mut coforged_tag = (**program).clone();
                coforged_tag.store.tags[source.instruction as usize] = FullTag::ExprParam;
                coforged_tag.store.generic.as_deref_mut().unwrap().test_lexical_capture_source_mut(id).unwrap().tag = FullTag::ExprParam;
                assert!(FullVerifier::verify(&coforged_tag).is_err(), "rewriting a port and its dependent receipt cannot change the original emission authority");
                let mut wrong_payload = (**program).clone();
                let range = wrong_payload.store.data[source.instruction as usize].range();
                wrong_payload.store.extra[range.start as usize] = source.slot + 1;
                assert!(FullVerifier::verify_lexical_capture_source(&wrong_payload.store, generic, &source).is_err());
                let mut coforged_type = (**program).clone();
                let other = generic.lexical_captures().find(|(_, other)| other.target == allocation.target && other.original_type != allocation.original_type).unwrap().1;
                let rewritten = coforged_type.store.generic.as_deref_mut().unwrap().test_lexical_capture_source_mut(id).unwrap();
                rewritten.ty = other.ty;
                rewritten.original_type = other.original_type.clone();
                let rewritten = coforged_type.store.generic.as_deref_mut().unwrap().test_lexical_capture_mut(source.capture).unwrap();
                rewritten.ty = other.ty;
                rewritten.original_type = other.original_type.clone();
                coforged_type.store.captures[allocation.header_index as usize].type_id = other.ty;
                assert!(FullVerifier::verify(&coforged_type).is_err(), "coforged scalar types cannot replace the original capture definition and read roots");
            }
        });
    });
}

#[test]
fn original_lexical_capture_pure_typed_uint_reads_live_binding_after_frontend_drop_on_both_routes() {
    run_eval(|| {
        for recursive in [false, true] {
            let (evaluator, plan) = prepare("var observed: UInt = 0\npure number() -> Int { let answer: Int = observed - 0; answer + 1 }\nprint ${number()}\nobserved = 4\nprint ${number()}\n");
            let program = evaluator.indexed_program.as_ref().unwrap();
            let symbols = program.symbol_owner().clone();
            assert!(program.generic_evidence().unwrap().lexical_capture_sources().any(|(_, source)| source.tag == FullTag::IntSlot && source.original_type == Type::UInt));
            let evaluate = || symbols.with_current(|| evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("the original typed UInt capture remains installed")));
            let output = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(evaluate) } else { evaluate() };
            assert!(output.diagnostics.is_empty(), "recursive={recursive}: {:?}", output.diagnostics);
            assert_eq!(output.status, 0);
            assert_eq!(output.stdout, b"1\n5\n");
        }
    });
}

#[test]
fn original_lexical_capture_refuses_missing_foreign_and_coforged_binding_slot_owner_receipts() {
    run_eval(|| {
        let source = "let base: Int = 3\nlet decoy: Int = 9\npure plus(value: Int) -> Int { value + base }\nlet alias = plus\n";
        let (evaluator, _) = prepare(source);
        let program = evaluator.indexed_program.as_ref().unwrap();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (read_id, original_read) = generic.lexical_capture_sources().next().unwrap();
            let read = original_read.clone();
            let capture = generic.lexical_capture(read.capture).unwrap().clone();
            let mut missing = (**program).clone();
            missing.store.generic.as_deref_mut().unwrap().test_clear_lexical_captures();
            assert!(FullVerifier::verify(&missing).is_err());
            let mut missing = (**program).clone();
            missing.store.generic.as_deref_mut().unwrap().test_clear_lexical_capture_sources();
            assert!(FullVerifier::verify(&missing).is_err());
            let (foreign, _) = prepare(source);
            let foreign_capture = foreign.indexed_program.as_ref().unwrap().generic_evidence().unwrap().lexical_captures().next().unwrap().0;
            assert!(generic.lexical_capture(foreign_capture).is_err());
            let mut replaced = (**program).clone();
            replaced.store.generic.as_deref_mut().unwrap().test_lexical_capture_source_mut(read_id).unwrap().capture = foreign_capture;
            assert!(FullVerifier::verify(&replaced).is_err());
            let decoy = generic.lexical_captures().find(|(_, candidate)| candidate.binding != capture.binding).unwrap().1.clone();
            for change in 0..3 {
                let mut changed = (**program).clone();
                let source = changed.store.generic.as_deref_mut().unwrap().test_lexical_capture_source_mut(read_id).unwrap();
                match change {
                    0 => source.slot = decoy.slot,
                    1 => source.binding = decoy.binding,
                    _ => source.owner = InstructionOwner::Driver(0),
                }
                let allocation = changed.store.generic.as_deref_mut().unwrap().test_lexical_capture_mut(read.capture).unwrap();
                match change {
                    0 => allocation.slot = decoy.slot,
                    1 => allocation.binding = decoy.binding,
                    _ => allocation.definition_owner = Some(allocation.declaration),
                }
                if change == 0 {
                    changed.store.captures[capture.header_index as usize].slot_and_flags = decoy.slot;
                    let range = changed.store.data[read.instruction as usize].range();
                    changed.store.extra[range.start as usize] = decoy.slot;
                }
                assert!(FullVerifier::verify(&changed).is_err(), "rewriting both dependent receipts cannot replace the original binding authority");
            }
            for change in 0..3 {
                let mut changed = (**program).clone();
                let header = &mut changed.store.captures[capture.header_index as usize];
                match change {
                    0 => header.slot_and_flags += 1,
                    1 => header.slot_and_flags |= 1 << 31,
                    _ => header.name = changed.store.functions[capture.target.index()].name,
                }
                assert!(FullVerifier::verify_lexical_capture(&changed.store, &capture).is_err());
                assert!(FullVerifier::verify(&changed).is_err());
            }
        });
    });
}

#[test]
fn original_lexical_capture_checkpoint_retires_allocations_without_reusing_their_authority() {
    run_eval(|| {
        let (evaluator, _) = prepare("let base: Int = 3\npure plus(value: Int) -> Int { value + base }\nlet alias = plus\n");
        let program = evaluator.indexed_program.as_ref().unwrap();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (_, source) = generic.lexical_capture_sources().next().unwrap();
            let original = generic.lexical_capture(source.capture).unwrap().clone();
            let mut builder = GenericEvidenceBuilder::default();
            let empty = builder.checkpoint();
            let retired = builder.add_lexical_capture(original.clone()).unwrap();
            let occupied = builder.checkpoint();
            builder.rewind(empty).unwrap();
            let replacement = builder.add_lexical_capture(original.clone()).unwrap();
            assert_ne!(retired, replacement);
            let mut read = source.clone();
            read.capture = retired;
            assert!(builder.add_lexical_capture_source(read.clone()).is_err());
            assert!(builder.rewind(occupied).is_err(), "a stale checkpoint cannot retire the replacement allocation");
            assert_eq!(builder.lexical_capture_for_slot(original.target, original.slot).unwrap().unwrap().0, replacement);
            read.capture = replacement;
            builder.add_lexical_capture_source(read).unwrap();
        });
    });
}
