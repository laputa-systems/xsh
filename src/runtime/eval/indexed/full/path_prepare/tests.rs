use super::*;
use crate::runtime::eval::{Evaluator, run_eval};
use crate::sema::check::Checker;
use crate::syntax::parser::Parser;

const SOURCE: &str = "pure selected(value: Path) -> Path { value }\nlet root = fp\"${args[0]}\"\nprint ${selected(root)}\n";

fn prepare(source: &str) -> (Evaluator, crate::runtime::eval::CompactIndexedRunPlan) {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("formatted-path.xsh", source);
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
        assert!(solved.upgrade().is_none());
        (evaluator, plan)
    })
}

#[test]
fn original_formatted_strings_keep_scalar_and_error_message_fragments_after_frontend_disposal() {
    run_eval(|| {
        for recursive in [false, true] {
            let source = "error Failure = Denied(message: Str)\npure selected(value: Str) -> Str { value }\npure floating(value: Float) -> Str { selected(f\"floating:${value:>5}\") }\npure counted(value: Int) -> Str { selected(f\"count:${value:>3}\") }\npure failed(value: Error) -> Str { selected(f\"failed: ${value.message}\") }\nprint ${floating(2.5)}\nprint ${counted(7)}\nprint ${failed(Failure.Denied(message: \"denied\"))}\n";
            let (evaluator, plan) = prepare(source);
            let program = evaluator.indexed_program.as_ref().unwrap();
            let symbols = program.symbol_owner().clone();
            let receipts = program.generic_evidence().unwrap().formatted_paths().collect::<Vec<_>>();
            assert_eq!(receipts.len(), 3);
            for (_, receipt) in receipts {
                assert_eq!(program.store.semantic.to_type(receipt.ty).unwrap(), Type::Str);
                assert!(FullVerifier::verify_formatted_path_operand(&program.store, program.generic_evidence().unwrap(), receipt.instruction, receipt.owner, &Type::Str, None, &mut Vec::new()).unwrap());
            }
            let output = run_eval(move || symbols.with_current(|| {
                crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("selected")), recursive, || {
                    evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("the original formatted strings remain installed"))
                })
            }));
            assert_eq!(output.status, 0, "{:?}", output.diagnostics);
            assert_eq!(output.stdout, b"floating:  2.5\ncount:  7\nfailed: denied\n");
            assert!(output.stderr.is_empty());
        }
    });
}

#[test]
fn original_generic_formatted_string_keeps_each_callers_display_witness_after_frontend_disposal() {
    run_eval(|| {
        for recursive in [false, true] {
            let source = "pure rendered(value) -> Str { f\"entry-${value}\" }\nprint ${rendered(7)}\nprint ${rendered(\"leaf\")}\n";
            let (evaluator, plan) = prepare(source);
            let program = evaluator.indexed_program.as_ref().unwrap();
            let symbols = program.symbol_owner().clone();
            assert_eq!(program.generic_evidence().unwrap().formatted_paths().count(), 1);
            let output = run_eval(move || symbols.with_current(|| {
                crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("rendered")), recursive, || {
                    evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("the original generic formatted string remains installed"))
                })
            }));
            assert_eq!(output.status, 0, "{:?}", output.diagnostics);
            assert_eq!(output.stdout, b"entry-7\nentry-leaf\n");
            assert!(output.stderr.is_empty());
        }
    });
}

#[test]
fn formatted_string_refuses_missing_foreign_wrong_target_and_same_typed_fragment_substitutions() {
    run_eval(|| {
        let source = "pure selected(value: Str) -> Str { value }\npure rendered(first: Str, second: Str) -> Str { selected(f\"${first}${second}\") }\nprint ${rendered(\"left\", \"right\")}\n";
        let (evaluator, _) = prepare(source);
        let program = evaluator.indexed_program.as_ref().unwrap();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, receipt) = generic.formatted_paths().next().unwrap();
            let receipt = receipt.clone();
            assert_eq!(program.store.semantic.to_type(receipt.ty).unwrap(), Type::Str);
            assert!(FullVerifier::verify_formatted_path_operand(&program.store, generic, receipt.instruction, receipt.owner, &Type::Path, None, &mut Vec::new()).is_err());

            let mut missing = (**program).clone();
            missing.store.generic.as_deref_mut().unwrap().test_clear_formatted_paths();
            assert!(FullVerifier::verify(&missing).is_err());

            let (foreign, _) = prepare(source);
            let foreign_id = foreign.indexed_program.as_ref().unwrap().generic_evidence().unwrap().formatted_paths().next().unwrap().0;
            assert!(generic.formatted_path(foreign_id).is_err());

            let mut target = (**program).clone();
            target.store.generic.as_deref_mut().unwrap().test_formatted_path_mut(id).unwrap().original.target = FormattedTarget::Path;
            assert!(FullVerifier::verify(&target).is_err());

            let (FormattedPathPart::Expression { instruction: first, ty: first_ty, .. }, FormattedPathPart::Expression { instruction: second, ty: second_ty, .. }) =
                (&receipt.parts[0], &receipt.parts[1]) else { panic!("two authored string fragments"); };
            assert_ne!(first, second);
            assert_eq!(first_ty, second_ty);
            assert_eq!(receipt.parts_payload[0..3], [2, 1, *first]);
            let range = program.store.blocks[IrBlockId::from_raw(receipt.parts_block).unwrap().index()].instructions;
            let mut substituted = (**program).clone();
            substituted.store.extra[range.start as usize + 2] = *second;
            assert!(FullVerifier::verify_formatted_path_source(&substituted.store, &receipt).is_err());

            let mut coforged = substituted;
            let copy = coforged.store.generic.as_deref_mut().unwrap().test_formatted_path_mut(id).unwrap();
            copy.parts_payload[2] = *second;
            copy.parts[0] = receipt.parts[1].clone();
            copy.original.parts[0] = receipt.original.parts[1].clone();
            assert!(FullVerifier::verify(&coforged).is_err(), "matching types and a matching rewritten recipe do not replace the authored fragment");

            let mut rewritten = (**program).clone();
            rewritten.store.tags[receipt.instruction as usize] = FullTag::ExprPathFmtString;
            assert!(FullVerifier::verify_formatted_path_source(&rewritten.store, &receipt).is_err());
            assert!(FullVerifier::verify(&rewritten).is_err());
        });
    });
}

#[test]
fn generic_formatted_string_refuses_foreign_caller_scopes_and_instances() {
    run_eval(|| {
        let source = "pure rendered(value) -> Str { f\"entry-${value}\" }\npure unrelated(value) { value }\nprint ${rendered(7)}\nlet other = unrelated(true)\n";
        let (evaluator, _) = prepare(source);
        let program = evaluator.indexed_program.as_ref().unwrap();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, receipt) = generic.formatted_paths().next().unwrap();
            let caller = receipt.caller.unwrap();
            let foreign_scope = generic.scopes().find(|(id, _)| *id != caller).unwrap().0;
            let foreign_instance = generic.instances().find(|(_, instance)| instance.scope == foreign_scope).unwrap().0;
            assert!(FullVerifier::verify_formatted_path_symbolic_operand(&program.store, generic, receipt.instruction, foreign_scope, TypeRef::Ground(receipt.ty), &mut vec![receipt.instruction]).is_err());
            assert!(FullVerifier::verify_formatted_path_operand(&program.store, generic, receipt.instruction, receipt.owner, &Type::Str, Some(foreign_instance), &mut vec![receipt.instruction]).is_err());
            let instance = generic.instances().find(|(_, instance)| instance.scope == caller).unwrap().0;
            assert!(FullVerifier::verify_formatted_path_operand(&program.store, generic, receipt.instruction, receipt.owner, &Type::Str, Some(instance), &mut vec![receipt.instruction]).unwrap());
            let mut changed = (**program).clone();
            changed.store.generic.as_deref_mut().unwrap().test_formatted_path_mut(id).unwrap().caller = Some(foreign_scope);
            assert!(FullVerifier::verify(&changed).is_err());
        });
    });
}

#[test]
fn original_generic_formatted_path_keeps_each_callers_interpolation_type_after_frontend_disposal() {
    run_eval(|| {
        for recursive in [false, true] {
            let source = "pure rendered(value) -> Path { fp\"entry-${value}\" }\nprint ${rendered(7)}\nprint ${rendered(\"leaf\")}\n";
            let (evaluator, plan) = prepare(source);
            let program = evaluator.indexed_program.as_ref().unwrap();
            let symbols = program.symbol_owner().clone();
            assert_eq!(program.generic_evidence().unwrap().formatted_paths().count(), 1);
            let output = run_eval(move || symbols.with_current(|| {
                crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("rendered")), recursive, || {
                    evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("the original generic formatted path remains installed"))
                })
            }));
            assert_eq!(output.status, 0, "{:?}", output.diagnostics);
            assert_eq!(output.stdout, b"entry-7\nentry-leaf\n");
            assert!(output.stderr.is_empty());
        }
    });
}

#[test]
fn generic_formatted_path_refuses_foreign_caller_scopes_and_instances() {
    run_eval(|| {
        let source = "pure rendered(value) -> Path { fp\"entry-${value}\" }\npure unrelated(value) { value }\nprint ${rendered(7)}\nlet other = unrelated(true)\n";
        let (evaluator, _) = prepare(source);
        let program = evaluator.indexed_program.as_ref().unwrap();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, path) = generic.formatted_paths().next().unwrap();
            let caller = path.caller.unwrap();
            assert!(matches!(path.parts[1], FormattedPathPart::Expression { ty: TypeRef::Rigid(_), .. }));
            let foreign_scope = generic.scopes().find(|(id, _)| *id != caller).unwrap().0;
            let foreign_instance = generic.instances().find(|(_, instance)| instance.scope == foreign_scope).unwrap().0;
            assert!(FullVerifier::verify_formatted_path_symbolic_operand(&program.store, generic, path.instruction, foreign_scope, TypeRef::Ground(path.ty), &mut vec![path.instruction]).is_err());
            assert!(FullVerifier::verify_formatted_path_operand(&program.store, generic, path.instruction, path.owner, &Type::Path, Some(foreign_instance), &mut vec![path.instruction]).is_err());
            let instance = generic.instances().find(|(_, instance)| instance.scope == caller).unwrap().0;
            assert!(FullVerifier::verify_formatted_path_operand(&program.store, generic, path.instruction, path.owner, &Type::Path, Some(instance), &mut vec![path.instruction]).unwrap());
            let mut changed = (**program).clone();
            changed.store.generic.as_deref_mut().unwrap().test_formatted_path_mut(id).unwrap().caller = Some(foreign_scope);
            assert!(FullVerifier::verify(&changed).is_err());
        });
    });
}

#[test]
fn original_formatted_path_enters_typed_calls_after_frontend_disposal_on_both_routes() {
    run_eval(|| {
        for recursive in [false, true] {
            let (evaluator, plan) = prepare(SOURCE);
            let program = evaluator.indexed_program.as_ref().unwrap();
            let symbols = program.symbol_owner().clone();
            let paths = program.generic_evidence().unwrap().formatted_paths().collect::<Vec<_>>();
            assert_eq!(paths.len(), 1);
            assert_eq!(program.store.semantic.to_type(paths[0].1.ty).unwrap(), Type::Path);
            assert_eq!(paths[0].1.parts.len(), 1);
            assert!(matches!(paths[0].1.parts[0], FormattedPathPart::Expression { .. }));
            let output = run_eval(move || symbols.with_current(|| {
                crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern("selected")), recursive, || {
                    evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("the original formatted path remains installed"))
                })
            }));
            assert_eq!(output.status, 0, "{:?}", output.diagnostics);
            assert_eq!(output.stdout, b"word\n");
            assert!(output.stderr.is_empty());
        }
    });
}

#[test]
fn original_formatted_path_preserves_text_integer_and_alignment_parts() {
    run_eval(|| {
        let source = "pure selected(value: Path) -> Path { value }\nlet count: Int = 7\nlet root = fp\"${args[0]}/entry-${count:>3}\"\nprint ${selected(root)}\n";
        let (evaluator, plan) = prepare(source);
        let program = evaluator.indexed_program.as_ref().unwrap();
        let symbols = program.symbol_owner().clone();
        let source = program.generic_evidence().unwrap().formatted_paths().next().unwrap().1;
        assert!(source.parts.iter().any(|part| matches!(part, FormattedPathPart::Text(text) if text.as_ref() == "/entry-")));
        assert!(source.parts.iter().any(|part| matches!(part, FormattedPathPart::Expression { ty: TypeRef::Ground(ty), .. } if program.store.semantic.to_type(*ty).unwrap() == Type::Int)));
        let output = run_eval(move || symbols.with_current(|| evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("the formatted parts remain installed"))));
        assert_eq!(output.status, 0, "{:?}", output.diagnostics);
        assert_eq!(output.stdout, b"word/entry-  7\n");
    });
}

#[test]
fn formatted_path_refuses_missing_foreign_rewritten_and_wrong_type_receipts() {
    run_eval(|| {
        let (evaluator, _) = prepare(SOURCE);
        let program = evaluator.indexed_program.as_ref().unwrap();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, source) = generic.formatted_paths().next().unwrap();
            let source = source.clone();
            assert!(FullVerifier::verify_formatted_path_operand(&program.store, generic, source.instruction, source.owner, &Type::Str, None, &mut Vec::new()).is_err());
            assert!(FullVerifier::verify_formatted_path_operand(&program.store, generic, source.instruction, source.owner, &Type::Optional(Box::new(Type::Path)), None, &mut Vec::new()).unwrap());
            let mut missing = (**program).clone();
            missing.store.generic.as_deref_mut().unwrap().test_clear_formatted_paths();
            assert!(FullVerifier::verify(&missing).is_err());
            let (foreign, _) = prepare(SOURCE);
            let foreign_id = foreign.indexed_program.as_ref().unwrap().generic_evidence().unwrap().formatted_paths().next().unwrap().0;
            assert!(generic.formatted_path(foreign_id).is_err());
            for change in 0..4 {
                let mut changed = (**program).clone();
                let receipt = changed.store.generic.as_deref_mut().unwrap().test_formatted_path_mut(id).unwrap();
                match change {
                    0 => receipt.instruction += 1,
                    1 => receipt.original.origin.source = SourceId::new(99),
                    2 => receipt.owner = InstructionOwner::Driver(u32::MAX),
                    _ => receipt.parts_payload[2] += 1,
                }
                assert!(FullVerifier::verify(&changed).is_err());
            }
            let mut wrong_kind = (**program).clone();
            wrong_kind.store.tags[source.instruction as usize] = FullTag::ExprFmtString;
            assert!(FullVerifier::verify_formatted_path_source(&wrong_kind.store, &source).is_err());
            assert!(FullVerifier::verify(&wrong_kind).is_err());
            let mut changed = (**program).clone();
            let range = changed.store.blocks[IrBlockId::from_raw(source.parts_block).unwrap().index()].instructions;
            assert_eq!(source.parts_payload[0..2], [1, 1]);
            changed.store.extra[range.start as usize + 2] += 1;
            assert_eq!(FullVerifier::verify_formatted_path_source(&changed.store, &source).unwrap_err().message, "formatted path changes its original formatting block or operands");
            let mut matched_copy = changed.clone();
            matched_copy.store.generic.as_deref_mut().unwrap().test_formatted_path_mut(id).unwrap().parts_payload[2] += 1;
            assert!(FullVerifier::verify(&matched_copy).is_err(), "a matching forged payload cannot replace the original source receipt");
        });
    });
}

#[test]
fn formatted_path_checkpoint_retires_ids_and_preserves_replacement_source_index() {
    run_eval(|| {
        let (evaluator, _) = prepare(SOURCE);
        let program = evaluator.indexed_program.as_ref().unwrap();
        let source = program.generic_evidence().unwrap().formatted_paths().next().unwrap().1.clone();
        let mut builder = GenericEvidenceBuilder::default();
        builder.register_instruction_origin(source.instruction, crate::runtime::eval::indexed::generic::OperationSourceOrigin::Expression(source.original.origin), source.owner).unwrap();
        for (part, original) in source.parts.iter().zip(source.original.parts.iter()) {
            if let (FormattedPathPart::Expression { source: instruction, .. }, OriginalPathPart::Expression { origin, .. }) = (part, original) {
                builder.register_instruction_origin(*instruction, crate::runtime::eval::indexed::generic::OperationSourceOrigin::Expression(*origin), source.owner).unwrap();
            }
        }
        let empty = builder.checkpoint();
        let retired = builder.add_formatted_path(source.clone()).unwrap();
        let old = builder.checkpoint();
        builder.rewind(empty).unwrap();
        let replacement = builder.add_formatted_path(source.clone()).unwrap();
        assert!(builder.rewind(old).is_err());
        let owners = program.store.generic_instruction_owners().unwrap();
        let store = builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap();
        assert!(store.formatted_path(retired).is_err());
        assert!(store.formatted_path(replacement).is_ok());
        assert_eq!(store.formatted_path_at(source.instruction).unwrap(), Some(replacement));
    });
}
