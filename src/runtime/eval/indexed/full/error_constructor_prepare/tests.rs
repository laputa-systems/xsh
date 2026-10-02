use super::*;
use crate::sema::check::Checker;

fn fixture(source: &str) -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "original-error-constructor.xsh", crate::loader::entry_source_from_text("original-error-constructor.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let solved = Arc::downgrade(&bodies.solved);
    let counters = bodies.solved.graph.counters().clone();
    let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
    assert_eq!(&counters, bodies.solved.graph.counters());
    drop(parsed); drop(declarations); drop(bodies);
    assert!(solved.upgrade().is_none());
    program
}

const SOURCE: &str = "error Failure = Failed(kind: Str, message: Str) : InvalidData | Other(kind: Str, message: Str)\npure first(message: Str) -> Failure { Failure.Failed(kind: \"failed\", message: message) }\npure second(message: Str) -> Failure { Failure.Other(kind: \"other\", message: message) }\n";

#[test]
fn original_error_constructor_embedded_json_module_keeps_canonical_family_after_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture("use json\nlet encoded = json.encode_lines([1])\n");
        FullVerifier::verify(&program).unwrap();
        program.symbol_owner().with_current(|| {
        let generic = program.store.generic.as_deref().unwrap();
        let original = generic.error_constructors().find(|source| source.original.member == Name::intern("Lines")).unwrap();
        assert_eq!(original.original.family, Name::intern("json.JsonError"));
        });
    });
}

#[test]
fn original_error_constructor_embedded_module_requires_its_original_checked_body_owner_and_application() {
    crate::runtime::eval::run_eval(|| {
        let source = "";
        let module_source = "error JsonError = Lines(kind: Str, message: Str)\npure lines_error(message: Str) -> JsonError { return JsonError.Lines(kind: \"type-error\", message: message) }\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("embedded-constructor-authority.xsh", source);
        let module_id = sources.add_file("embedded-errors.xsh", module_source);
        let mut builder = crate::syntax::arena::ArenaProgramBuilder::with_token_capacity(128);
        let entry = crate::syntax::parser::Parser::parse_source_into_arena_builder(source_id, source, &mut builder);
        let module = crate::syntax::parser::Parser::parse_source_into_arena_builder(module_id, module_source, &mut builder);
        assert!(entry.diagnostics.is_empty() && module.diagnostics.is_empty());
        let namespace = builder.symbol_owner().with_current(|| Name::intern("json"));
        builder.push_internal_arena_module("json".to_owned(), namespace, module.statements);
        let parsed = builder.finish_with_statements(entry.statements);
        parsed.symbol_owner().with_current(|| {
        Checker::reset_module_reuse_counters();
        let declarations = Checker::check_compact_declarations(&parsed);
        let bodies = Checker::probe_compact_bodies(&parsed, &declarations);
        assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let (origin, application) = bodies.solved.constructor_applications.iter().find(|(_, application)| matches!(
            super::super::super::generic::graph_ground_type(&bodies.solved.graph, application.result), Ok(Type::ErrorVariant { variant, .. }) if variant == "Lines")).unwrap();
        let origin = *origin;
        let owner = application.caller.unwrap();
        assert_eq!(origin.namespace, Some(Name::intern("json")));
        assert_eq!(owner.namespace, origin.namespace);
        assert_eq!(bodies.solved.expression_owners.get(&origin), Some(&owner));
        let counters = Checker::module_reuse_counters();
        assert_eq!(counters.declaration_checks.get(&owner), Some(&1));
        assert_eq!(counters.declaration_generations.get(&owner), Some(&1));
        let sources = Arc::new(sources);
        let original = FullBuilder::build_compact(&parsed, &declarations, &bodies, source, Arc::clone(&sources), source_id).unwrap();
        FullVerifier::verify(&original).unwrap();
        for remove_application in [false, true] {
            let mut changed_declarations = Checker::check_compact_declarations(&parsed);
            let solved = Arc::get_mut(&mut changed_declarations.solved).expect("the isolated checked declaration graph has one owner before the body probe");
            if remove_application { solved.constructor_applications.remove(&origin); }
            else { solved.expression_owners.insert(origin, crate::sema::check::DeclarationIdentity { namespace: None, ..owner }); }
            let changed = Checker::probe_compact_bodies(&parsed, &changed_declarations);
            assert!(FullBuilder::build_compact(&parsed, &changed_declarations, &changed, source, Arc::clone(&sources), source_id).is_err(), "an embedded constructor requires its original body owner and application");
        }
        });
    });
}

#[test]
fn original_error_constructor_import_alias_keeps_canonical_declaration_family_after_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let source = "use model as selected\nlet failure = selected.made(\"kept\")\n";
        let module_source = "##! Declared module errors.\nerror Failure = Failed(message: Str) : InvalidData\n## Construct a declared nominal failure.\nexport pure made(message: Str) -> Error { Failure.Failed(message: message) }\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("imported-error.xsh", source);
        let module_id = sources.add_file("error-model.xsh", module_source);
        let mut builder = crate::syntax::arena::ArenaProgramBuilder::with_token_capacity(128);
        let entry = crate::syntax::parser::Parser::parse_source_into_arena_builder(source_id, source, &mut builder);
        let module = crate::syntax::parser::Parser::parse_source_into_arena_builder(module_id, module_source, &mut builder);
        assert!(entry.diagnostics.is_empty() && module.diagnostics.is_empty());
        let namespace = builder.symbol_owner().with_current(|| Name::intern("error-model"));
        for statement in builder.statement_ids(entry.statements) {
            if let Some((import, _, _)) = builder.use_stmt_for_statement(statement) { builder.set_use_resolved(import, Arc::from("error-model")); }
        }
        builder.push_arena_module("error-model".to_owned(), namespace, module.statements);
        let parsed = builder.finish_with_statements(entry.statements);
        let checked = Checker::check_arena(&parsed, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let solved = Arc::downgrade(&checked.solved);
        let declarations = Checker::compact_declarations_from_checked(&parsed, &checked);
        let bodies = Checker::probe_compact_bodies(&parsed, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let program = FullBuilder::build_compact(&parsed, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
        drop(parsed); drop(checked); drop(declarations); drop(bodies);
        assert!(solved.upgrade().is_none());
        program.symbol_owner().with_current(|| {
            let original = program.store.generic.as_deref().unwrap().error_constructors().next().unwrap();
            assert!(matches!(original.original.authority, crate::sema::check::QualifiedNominalIdentity::Source { source, namespace: Some(owner), .. } if source == module_id && owner == namespace));
            assert_eq!(original.original.family, Name::intern("error-model.Failure"));
            assert_eq!(program.store.string(original.payload[1]).unwrap(), "error-model.Failure");
            FullVerifier::verify(&program).unwrap();
        });
    });
}

#[test]
fn original_error_constructor_retains_canonical_member_fields_and_facets_after_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture(SOURCE);
        program.symbol_owner().with_current(|| {
        let generic = program.store.generic.as_deref().unwrap();
        assert_eq!(generic.error_constructors().count(), 2);
        let original = generic.error_constructors().find(|source| source.original.member == Name::intern("Failed")).unwrap();
        assert_eq!(original.original.facets.as_ref(), &[Name::intern("InvalidData")]);
        assert_eq!(original.original.parameters.iter().map(|(name, _)| *name).collect::<Vec<_>>(), vec![Name::intern("kind"), Name::intern("message")]);
        FullVerifier::verify(&program).unwrap();
        });
    });
}

#[test]
fn original_error_constructor_refuses_missing_foreign_and_rewritten_nominal_payload_authority() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture(SOURCE);
        program.symbol_owner().with_current(|| {
        let source = program.store.generic.as_deref().unwrap().error_constructors().find(|source| source.original.member == Name::intern("Failed")).unwrap().clone();
        let mut missing = program.clone();
        missing.store.generic.as_deref_mut().unwrap().test_remove_error_constructors();
        assert!(FullVerifier::verify(&missing).is_err());
        let other = fixture(SOURCE);
        let mut foreign = program.clone();
        foreign.store.generic.as_deref_mut().unwrap().test_replace_error_constructors(other.store.generic.as_deref().unwrap());
        assert!(FullVerifier::verify(&foreign).unwrap_err().message.contains("foreign program"));
        let replacement = program.store.generic.as_deref().unwrap().error_constructors().find(|other| other.instruction != source.instruction).unwrap();
        let mut renamed = program.clone();
        let raw = renamed.store.data[source.instruction as usize].range().start as usize;
        renamed.store.extra[raw + 2] = replacement.payload[2];
        assert!(FullVerifier::verify(&renamed).is_err(), "an equal-signature member cannot replace the selected variant");
        let mut rewritten = program.clone();
        rewritten.store.generic.as_deref_mut().unwrap().test_error_constructor_mut(source.instruction).unwrap().original.authority = replacement.original.authority;
        assert!(FullVerifier::verify(&rewritten).is_err());
        let mut fields = program.clone();
        let block = fields.store.blocks[IrBlockId::from_raw(source.field_block.0).unwrap().index()].instructions.start as usize;
        fields.store.extra.swap(block + 2, block + 4);
        assert!(FullVerifier::verify(&fields).is_err(), "equal-type payload fields retain their original operand slots");
        let mut label = program.clone();
        label.store.extra[block + 1] = label.store.extra[block + 3];
        assert!(FullVerifier::verify(&label).is_err());
        });
    });
}

#[test]
fn original_error_constructor_cause_translation_refuses_a_same_owner_non_error_operand() {
    crate::runtime::eval::run_eval(|| {
        let source = "error Outer = Failed(message: Str)\nerror Inner = Failed(message: Str)\npure translated() -> Result[Unit, Outer] { Err(Outer.Failed(message: \"outer\"), cause: Inner.Failed(message: \"inner\")) }\n";
        let program = fixture(source);
        let instruction = program.store.tags.iter().position(|tag| *tag == FullTag::ExprErr).unwrap();
        let string = program.store.tags.iter().position(|tag| *tag == FullTag::ExprStr).unwrap();
        let mut changed = program.clone();
        let raw = changed.store.data[instruction].range().start as usize;
        assert_eq!(changed.store.extra[raw + 1], 1);
        changed.store.extra[raw + 2] = string as u32;
        assert!(FullVerifier::verify(&changed).is_err(), "a valid local Str source cannot replace the original typed cause");
    });
}

#[test]
fn original_error_constructor_evaluates_fields_in_source_order_in_both_disposed_frontend_routes() {
    crate::runtime::eval::run_eval(|| {
        let source = "error Failure = Failed(kind: Str, message: Str) : InvalidData\nproc marker(label: Str) [io] -> Str { print $label; label }\nproc made() [io] -> Failure { Failure.Failed(message: marker(\"message\"), kind: marker(\"kind\")) }\nlet value = made()\nmatch value { Failure.Failed {kind, message} => print $kind $message; _ => print \"wrong\" }\n";
        for recursive in [false, true] {
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("original-error-execution.xsh", source);
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let solved = Arc::downgrade(&checked.solved);
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
            let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
            drop(parsed); drop(checked);
            assert!(solved.upgrade().is_none());
            let symbols = evaluator.indexed_program.as_ref().unwrap().symbol_owner().clone();
            let execute = || symbols.with_current(|| {
                assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("prepared indexed program remains installed"))
            });
            let output = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute) } else { execute() };
            assert_eq!(output.status, 0, "{:?}; {:?}", output.diagnostics, output.traceback);
            assert_eq!(output.stdout, b"message\nkind\nkind message\n");
            assert!(output.stderr.is_empty());
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        }
    });
}
