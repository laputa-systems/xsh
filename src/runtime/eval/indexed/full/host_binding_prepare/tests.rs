use super::*;
use crate::runtime::eval::{Evaluator, run_eval};
use crate::sema::check::Checker;
use crate::syntax::parser::Parser;

const SOURCE: &str = "pure selected(value: Str) -> Str { value }\nlet word = args[0]\nprint ${selected(word)}\n";

fn prepare(source: &str) -> (Evaluator, crate::runtime::eval::CompactIndexedRunPlan) {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("host-args.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let symbols = parsed.arena.symbol_owner().clone();
    symbols.with_current(|| {
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let solved = Arc::downgrade(&checked.solved);
        let mut evaluator = Evaluator::new_with_sources(vec!["word".to_string()], sources);
        let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
        drop(checked); drop(parsed);
        assert!(solved.upgrade().is_none(), "host hydration retains no checker or inference bundle");
        (evaluator, plan)
    })
}

#[test]
fn original_host_args_execute_after_frontend_disposal_on_both_routes() {
    run_eval(|| {
        for recursive in [false, true] {
            let (evaluator, plan) = prepare(SOURCE);
            let program = evaluator.indexed_program.as_ref().unwrap();
            let symbols = program.symbol_owner().clone();
            let sources = program.generic_evidence().unwrap().host_binding_sources().collect::<Vec<_>>();
            assert_eq!(sources.len(), 1);
            assert_eq!(program.store.semantic.to_type(sources[0].1.ty).unwrap(), Type::List(Box::new(Type::Str)));
            let output = run_eval(move || symbols.with_current(|| {
                crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("selected")), recursive, || {
                    evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("the prepared host binding remains installed"))
                })
            }));
            assert_eq!(output.status, 0, "{:?}", output.diagnostics);
            assert_eq!(output.stdout, b"word\n");
            assert!(output.stderr.is_empty());
        }
    });
}

#[test]
fn captured_host_args_keep_the_original_environment_after_frontend_disposal_on_both_routes() {
    run_eval(|| {
        let source = "pure selected() -> Str { args[0] }\npure shadowed() -> Str { let args = [\"shadow\"]; selected() }\nlet word = args[0]\nprint ${shadowed()}\n";
        for recursive in [false, true] {
            let (evaluator, plan) = prepare(source);
            let program = evaluator.indexed_program.as_ref().unwrap();
            let symbols = program.symbol_owner().clone();
            let sources = program.generic_evidence().unwrap().host_binding_sources().collect::<Vec<_>>();
            assert_eq!(sources.len(), 2);
            assert!(sources.iter().any(|(_, source)| matches!(source.owner, InstructionOwner::Function(_))));
            let output = run_eval(move || symbols.with_current(|| {
                crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("selected")), recursive, || {
                    evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("the captured host binding remains installed"))
                })
            }));
            assert_eq!(output.status, 0, "{:?}", output.diagnostics);
            assert_eq!(output.stdout, b"word\n");
            assert!(output.stderr.is_empty());
        }
    });
}

#[test]
fn saved_callable_host_args_keep_the_original_creation_environment_on_both_routes() {
    run_eval(|| {
        let source = "pure selected() -> Str { args[0] }\npure shadowed() -> Str { let args = [\"shadow\"]; let saved = selected; saved() }\nlet word = args[0]\nprint ${shadowed()}\n";
        for recursive in [false, true] {
            let (evaluator, plan) = prepare(source);
            let symbols = evaluator.indexed_program.as_ref().unwrap().symbol_owner().clone();
            let output = run_eval(move || symbols.with_current(|| {
                crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("selected")), recursive, || {
                    evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("the saved host environment remains installed"))
                })
            }));
            assert_eq!(output.status, 0, "{:?}", output.diagnostics);
            assert_eq!(output.stdout, b"word\n");
            assert!(output.stderr.is_empty());
        }
    });
}

#[test]
fn captured_host_args_refuse_foreign_capture_headers_and_captured_slot_writes() {
    run_eval(|| {
        let source = "pure selected() -> Str { var other: Str = \"original\"; other = \"changed\"; args[0] }\nlet word = args[0]\nprint ${selected()}\n";
        let (evaluator, _) = prepare(source);
        let program = evaluator.indexed_program.as_ref().unwrap();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (source_id, receipt) = generic.host_binding_sources().find(|(_, source)| matches!(source.owner, InstructionOwner::Function(_))).unwrap();
            let receipt = receipt.clone();
            let capture_id = receipt.capture.unwrap();
            let capture = generic.host_binding_capture(capture_id).unwrap().clone();
            let mut missing = (**program).clone();
            missing.store.generic.as_deref_mut().unwrap().test_clear_host_binding_captures();
            assert!(FullVerifier::verify(&missing).is_err());
            let mut rewritten = (**program).clone();
            rewritten.store.generic.as_deref_mut().unwrap().test_host_binding_capture_mut(capture_id).unwrap().slot += 1;
            assert!(FullVerifier::verify(&rewritten).is_err(), "an equal replacement ledger does not preserve the original allocation receipt");
            let (foreign, _) = prepare(source);
            let foreign_id = foreign.indexed_program.as_ref().unwrap().generic_evidence().unwrap().host_binding_captures().next().unwrap().0;
            assert!(generic.host_binding_capture(foreign_id).is_err());
            let mut foreign_read = (**program).clone();
            foreign_read.store.generic.as_deref_mut().unwrap().test_host_binding_source_mut(source_id).unwrap().capture = Some(foreign_id);
            assert!(FullVerifier::verify(&foreign_read).is_err());
            for change in 0..3 {
                let mut changed = (**program).clone();
                let header = &mut changed.store.captures[capture.header_index as usize];
                match change {
                    0 => header.slot_and_flags |= 1 << 31,
                    1 => header.slot_and_flags += 1,
                    _ => header.name = changed.store.functions[capture.target.index()].name,
                }
                assert!(FullVerifier::verify_host_binding_capture(&changed.store, &capture).is_err());
                assert!(FullVerifier::verify(&changed).is_err());
            }
            let mut changed = (**program).clone();
            let assignment = changed.store.function_instruction_range(capture.target.index()).unwrap().find(|&index| changed.store.tags[index] == FullTag::StmtAssign).unwrap();
            let range = changed.store.data[assignment].range();
            changed.store.extra[range.start as usize] = capture.slot;
            assert_eq!(FullVerifier::verify_host_binding_source(&changed.store, &receipt).unwrap_err().message, "host capture has an unprepared write in its function scope");
            assert!(FullVerifier::verify(&changed).is_err());
        });
    });
}

#[test]
fn original_host_args_reject_missing_foreign_rewritten_slot_scope_and_write_receipts() {
    run_eval(|| {
        let (evaluator, _) = prepare(SOURCE);
        let program = evaluator.indexed_program.as_ref().unwrap();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, source) = generic.host_binding_sources().next().unwrap();
            let source = source.clone();
            let mut missing = (**program).clone();
            missing.store.generic.as_deref_mut().unwrap().test_clear_host_bindings();
            assert!(FullVerifier::verify(&missing).is_err());
            let (foreign, _) = prepare(SOURCE);
            let foreign_id = foreign.indexed_program.as_ref().unwrap().generic_evidence().unwrap().host_binding_sources().next().unwrap().0;
            assert!(generic.host_binding_source(foreign_id).is_err());
            for change in 0..4 {
                let mut changed = (**program).clone();
                let row = changed.store.generic.as_deref_mut().unwrap().test_host_binding_source_mut(id).unwrap();
                match change {
                    0 => row.slot += 1,
                    1 => row.scope_start = row.scope_end,
                    2 => row.origin.source = SourceId::new(99),
                    _ => row.owner = InstructionOwner::Driver(source.scope_end),
                }
                assert!(FullVerifier::verify(&changed).is_err());
            }
            let InstructionOwner::Driver(step) = source.owner else { unreachable!() };
            let mut rewritten = (**program).clone();
            let slots = rewritten.store.driver_steps[step as usize].slots.bounds(rewritten.store.driver_slots.len()).unwrap();
            let slot = rewritten.store.driver_slots[slots].iter_mut().find(|slot| slot.slot == source.slot).unwrap();
            slot.flags |= DRIVER_SLOT_MUTABLE | DRIVER_SLOT_WRITE;
            assert!(FullVerifier::verify(&rewritten).is_err());
            let mut jointly_rewritten = rewritten.clone();
            jointly_rewritten.store.generic.as_deref_mut().unwrap().test_host_binding_source_mut(id).unwrap().slot_flags |= DRIVER_SLOT_MUTABLE | DRIVER_SLOT_WRITE;
            assert!(FullVerifier::verify(&jointly_rewritten).is_err(), "matching mutable copies cannot replace the original host receipt");
        });
    });
}

#[test]
fn a_local_args_shadow_keeps_its_authored_binding_and_no_host_receipt() {
    run_eval(|| {
        let source = "pure selected(value: Str) -> Str { let args = [value]; args[0] }\nlet word = args[0]\nprint ${selected(word)}\n";
        let (evaluator, plan) = prepare(source);
        let program = evaluator.indexed_program.as_ref().unwrap();
        let symbols = program.symbol_owner().clone();
        let sources = program.generic_evidence().unwrap().host_binding_sources().collect::<Vec<_>>();
        assert_eq!(sources.len(), 1);
        assert!(matches!(sources[0].1.owner, InstructionOwner::Driver(_)));
        let output = run_eval(move || symbols.with_current(|| evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("the shadowed source remains prepared"))));
        assert_eq!(output.status, 0, "{:?}", output.diagnostics);
        assert_eq!(output.stdout, b"word\n");
    });
}

#[test]
fn host_args_refuse_an_authored_driver_declaration_or_write_retargeted_to_the_seed() {
    run_eval(|| {
        let source = "pure selected(value: Str) -> Str { value }\nvar other: Str = \"original\"\nother = \"changed\"\nlet word = args[0]\nprint ${selected(word)}\n";
        let (evaluator, _) = prepare(source);
        let program = evaluator.indexed_program.as_ref().unwrap();
        program.symbol_owner().with_current(|| {
            let receipt = program.generic_evidence().unwrap().host_binding_sources().next().unwrap().1.clone();
            let InstructionOwner::Driver(read_step) = receipt.owner else { unreachable!() };
            for kind in [FullDriverTag::Let, FullDriverTag::Assign] {
                let mut changed = (**program).clone();
                let step = changed.store.driver_steps[receipt.scope_start as usize..read_step as usize].iter().find(|step| step.tag == kind).unwrap();
                let range = step.data.range();
                changed.store.extra[range.start as usize] = receipt.binding.name().symbol().raw();
                let error = FullVerifier::verify_host_binding_source(&changed.store, &receipt).unwrap_err();
                assert_eq!(error.message, "host binding was replaced or written before its read");
                assert!(FullVerifier::verify(&changed).is_err());
            }
        });
    });
}

#[test]
fn host_binding_rewind_retires_source_ids_and_prevalidates_replacement_checkpoints() {
    run_eval(|| {
        let (evaluator, _) = prepare(SOURCE);
        let program = evaluator.indexed_program.as_ref().unwrap();
        let source = program.generic_evidence().unwrap().host_binding_sources().next().unwrap().1.clone();
        let mut builder = GenericEvidenceBuilder::default();
        builder.register_instruction_origin(source.instruction, crate::runtime::eval::indexed::generic::OperationSourceOrigin::Expression(source.origin), source.owner).unwrap();
        let empty = builder.checkpoint();
        let retired = builder.add_host_binding_source(source.clone()).unwrap();
        let old = builder.checkpoint();
        builder.rewind(empty).unwrap();
        let replacement = builder.add_host_binding_source(source.clone()).unwrap();
        assert!(builder.rewind(old).is_err(), "a reused position cannot authorize a retired checkpoint");
        let owners = program.store.generic_instruction_owners().unwrap();
        let store = builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap();
        assert!(store.host_binding_source(retired).is_err());
        assert!(store.host_binding_source(replacement).is_ok(), "rejected rewind leaves the replacement source intact");
        assert_eq!(store.host_binding_source_at(source.instruction).unwrap(), Some(replacement));
    });
}

#[test]
fn host_capture_rewind_retires_environment_ids_and_restores_the_slot_index() {
    run_eval(|| {
        let (evaluator, _) = prepare(SOURCE);
        let program = evaluator.indexed_program.as_ref().unwrap();
        let capture = program.generic_evidence().unwrap().host_binding_captures().next().unwrap().1.clone();
        let mut builder = GenericEvidenceBuilder::default();
        let empty = builder.checkpoint();
        let retired = builder.add_host_binding_capture(capture.clone()).unwrap();
        let old = builder.checkpoint();
        builder.rewind(empty).unwrap();
        assert!(builder.host_binding_capture_for_slot(capture.target, capture.slot).unwrap().is_none());
        let replacement = builder.add_host_binding_capture(capture.clone()).unwrap();
        assert!(builder.rewind(old).is_err());
        assert_eq!(builder.host_binding_capture_for_slot(capture.target, capture.slot).unwrap().unwrap().0, replacement);
        let owners = program.store.generic_instruction_owners().unwrap();
        let store = builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap();
        assert!(store.host_binding_capture(retired).is_err());
        assert!(store.host_binding_capture(replacement).is_ok());
        assert_eq!(store.host_binding_capture_for_slot(capture.target, capture.slot).unwrap().unwrap().0, replacement);
    });
}

#[test]
fn a_declarationless_wrapper_omits_an_unused_host_capture_without_compacting_slots() {
    run_eval(|| {
        let source = "pure selected() -> Str { \"fixed\" }\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("unused-host-wrapper.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty());
        parsed.arena.symbol_owner().with_current(|| {
            let declarations = Checker::check_compact_declarations(&parsed.arena);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
            let mut builder = FullBuilder::new(source_id);
            builder.solved = Some(Arc::clone(&bodies.solved));
            builder.reserve_function_keys(crate::runtime::eval::lower::compact_function_keys(&parsed.arena)).unwrap();
            crate::runtime::eval::lower::lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                crate::runtime::eval::lower::StdlibLowerLinkage::Local, |mut unit| {
                    let body = unit.body.as_mut().unwrap();
                    assert_eq!(body.captures.iter().filter(|capture| capture.host_binding.is_some()).count(), 1);
                    body.solved_declaration = None;
                    body.legacy_checked_signature = Some((Vec::new(), Type::Str));
                    builder.predeclare(&[&unit])?;
                    let body = unit.take_lowered_body().unwrap();
                    let function = builder.function_ids[&unit.key()];
                    builder.current_owner = Some(function.raw());
                    builder.current_slot_count = body.slot_count as u32;
                    builder.encode_body(function, &body)?;
                    let header = builder.store.functions[function.index()];
                    assert!(header.captures.bounds(builder.store.captures.len()).unwrap().is_empty());
                    assert_eq!(header.slot_count as usize, body.slot_count, "the original unused host slot remains reserved");
                    assert_eq!(header.slot_count, 1);
                    assert!(builder.generic.as_ref().is_none_or(|generic| generic.host_binding_capture_for_slot(function, 0).unwrap().is_none()));
                    Ok(())
                }).unwrap();
        });
    });
}

#[test]
fn a_used_nested_host_slot_cannot_be_published_as_a_declarationless_wrapper() {
    run_eval(|| {
        let source = "pure selected() -> Str { args[0] }\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("used-host-wrapper.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty());
        parsed.arena.symbol_owner().with_current(|| {
            let declarations = Checker::check_compact_declarations(&parsed.arena);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
            let mut builder = FullBuilder::new(source_id);
            builder.solved = Some(Arc::clone(&bodies.solved));
            builder.reserve_function_keys(crate::runtime::eval::lower::compact_function_keys(&parsed.arena)).unwrap();
            let error = crate::runtime::eval::lower::lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources,
                crate::runtime::eval::lower::StdlibLowerLinkage::Local, |mut unit| {
                    let body = unit.body.as_mut().unwrap();
                    body.solved_declaration = None;
                    body.legacy_checked_signature = Some((Vec::new(), Type::Str));
                    builder.predeclare(&[&unit])?;
                    let body = unit.take_lowered_body().unwrap();
                    let function = builder.function_ids[&unit.key()];
                    builder.current_owner = Some(function.raw());
                    builder.current_slot_count = body.slot_count as u32;
                    builder.encode_body(function, &body)
                }).unwrap_err();
            assert_eq!(error.construct, "host_capture_original_declaration_missing");
            assert!(builder.encoded_slot_uses.iter().any(|(_, slot)| *slot == 0), "the nested Args index operand is an actual encoded slot use");
        });
    });
}
