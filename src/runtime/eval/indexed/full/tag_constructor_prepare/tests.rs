use super::*;
use crate::sema::check::Checker;

fn fixture(source: &str) -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "original-tag-constructor.xsh", crate::loader::entry_source_from_text("original-tag-constructor.xsh", source.to_owned()), Vec::new());
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

const SOURCE: &str = "enum Choice { First(Str, Str), Second(Str, Str), Empty }\npure first(value: Str) -> Choice { First(value, \"first\") }\npure second(value: Str) -> Choice { Second(value, \"second\") }\npure empty() -> Choice { Empty }\n";

#[test]
fn original_tag_constructor_keeps_actual_checked_member_and_payload_after_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture(SOURCE);
        program.symbol_owner().with_current(|| {
            let generic = program.store.generic.as_deref().unwrap();
            assert_eq!(generic.tag_constructors().count(), 3);
            let first = generic.tag_constructors().find(|source| source.original.member == Name::intern("First")).unwrap();
            assert_eq!(first.original.parameters.as_ref(), &[Type::Str, Type::Str]);
            assert!(first.original.application.requirement.is_some());
            assert_eq!(first.original.application.supplied.len(), 2);
            FullVerifier::verify(&program).unwrap();
        });
    });
}

#[test]
fn original_tag_constructor_refuses_missing_foreign_member_payload_and_default_receipts() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture(SOURCE);
        program.symbol_owner().with_current(|| {
            let source = program.store.generic.as_deref().unwrap().tag_constructors().find(|source| source.original.member == Name::intern("First")).unwrap().clone();
            let replacement = program.store.generic.as_deref().unwrap().tag_constructors().find(|source| source.original.member == Name::intern("Second")).unwrap();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_tag_constructors();
            assert!(FullVerifier::verify(&missing).is_err());
            let other = fixture(SOURCE);
            let mut foreign = program.clone();
            foreign.store.generic.as_deref_mut().unwrap().test_replace_tag_constructors(other.store.generic.as_deref().unwrap());
            assert!(FullVerifier::verify(&foreign).unwrap_err().message.contains("foreign program"));
            let mut member = program.clone();
            let start = member.store.data[source.instruction as usize].range().start as usize;
            member.store.extra[start + 1] = replacement.payload[1];
            assert!(FullVerifier::verify(&member).is_err(), "equal payload signatures cannot replace the selected member");
            let mut owner = program.clone();
            owner.store.generic.as_deref_mut().unwrap().test_tag_constructor_mut(source.instruction).unwrap().original.authority = replacement.original.authority;
            assert!(FullVerifier::verify(&owner).is_err());
            let mut fields = program.clone();
            let start = fields.store.blocks[IrBlockId::from_raw(source.field_block.0).unwrap().index()].instructions.start as usize;
            fields.store.extra.swap(start + 1, start + 2);
            assert!(FullVerifier::verify(&fields).is_err(), "equal payload types cannot exchange original source order");
            let mut defaults = program.clone();
            defaults.store.generic.as_deref_mut().unwrap().test_tag_constructor_mut(source.instruction).unwrap().original.application.default_slots.push(0);
            assert!(FullVerifier::verify(&defaults).is_err(), "tag payloads do not acquire record defaults");
        });
    });
}

#[test]
fn original_tag_constructor_retains_original_wire_owner_after_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture("enum State: Str { Seen = \"seen\", Missing = \"missing\" }\npure selected() -> State { Seen }\n");
        program.symbol_owner().with_current(|| {
            FullVerifier::verify(&program).unwrap();
            let source = program.store.generic.as_deref().unwrap().tag_constructors().next().unwrap();
            assert!(Arc::ptr_eq(source.original.wire.as_ref().unwrap(), &program.store.wire_enums[0]));
            let mut changed = program.clone();
            changed.store.wire_enums[0] = Arc::new((*changed.store.wire_enums[0]).clone());
            assert!(FullVerifier::verify(&changed).is_err(), "an equal wire mapping cannot replace its original owner");
        });
    });
}
#[test]
fn original_tag_constructor_evaluates_payload_in_source_order_in_both_disposed_frontend_routes() {
    crate::runtime::eval::run_eval(|| {
        let source = "enum Choice { Made(Str, Str) }\nproc marker(label: Str) [io] -> Str { print $label; label }\nproc made() [io] -> Choice { Made(marker(\"first\"), marker(\"second\")) }\nlet value = made()\nmatch value { Made(first, second) => print $first $second }\n";
        for recursive in [false, true] {
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("original-tag-execution.xsh", source);
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
            assert_eq!(output.stdout, b"first\nsecond\nfirst second\n");
            assert!(output.stderr.is_empty());
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        }
    });
}

#[test]
fn original_tag_constructor_checks_creation_before_saved_payload_initialization() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture(SOURCE);
        program.symbol_owner().with_current(|| {
            let original = program.store.generic.as_deref().unwrap().tag_constructors().find(|source| source.original.member == Name::intern("First")).unwrap();
            let InstructionOwner::Function(function) = original.owner else { panic!("fixture constructor belongs to its original checked function"); };
            let instruction = original.instruction;
            let wrapper = original.argument_wrappers[0];
            program.function_view_by_id(function).unwrap().execution().unwrap().tag_constructor_argument_wrapper(wrapper).unwrap();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_tag_constructors();
            let execution = missing.function_view_by_id(function).unwrap().execution().unwrap();
            assert!(execution.tag_constructor(instruction).is_err());
            assert!(execution.tag_constructor_argument_wrapper(wrapper).is_err(), "missing original creation authority refuses before the saved payload initializer");
            let mut fields = program.clone();
            let start = fields.store.blocks[IrBlockId::from_raw(original.field_block.0).unwrap().index()].instructions.start as usize;
            fields.store.extra.swap(start + 1, start + 2);
            assert!(fields.function_view_by_id(function).unwrap().execution().unwrap().tag_constructor_argument_wrapper(wrapper).is_err(), "substituted payload allocation refuses before original operand effects");
        });
    });
}

#[test]
fn original_tag_constructor_bare_tail_keeps_statement_authority_after_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture("enum Choice { Empty }\npure selected() -> Choice { Empty }\n");
        program.symbol_owner().with_current(|| {
            let source = program.store.generic.as_deref().unwrap().tag_constructors().next().unwrap();
            assert!(matches!(source.original.origin, OperationSourceOrigin::Statement(_)), "a bare tail retains its authored statement identity");
            assert!(source.fields.is_empty());
            FullVerifier::verify(&program).unwrap();
        });
    });
}

#[test]
fn original_tag_constructor_wire_owner_is_independent_of_legacy_declaration_mapping() {
    crate::runtime::eval::run_eval(|| {
        let source = "enum State: Str { Seen = \"seen\", Missing = \"missing\" }\npure selected() -> State { Seen }\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("original-tag-wire-source.xsh", source);
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, source);
        let mut declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(declarations.diagnostics.is_empty() && bodies.diagnostics.is_empty());
        declarations.wire_enums.mappings.clear();
        let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
        drop(parsed); drop(declarations); drop(bodies);
        program.symbol_owner().with_current(|| {
            FullVerifier::verify(&program).unwrap();
            let original = program.store.generic.as_deref().unwrap().tag_constructors().next().unwrap();
            assert!(Arc::ptr_eq(original.original.wire.as_ref().unwrap(), &program.store.wire_enums[0]), "the checked nominal source supplies its original mapping after legacy metadata erasure");
        });
    });
}

#[test]
fn scoped_tag_constructor_original_parameter_instances_survive_frontend_drop_on_both_workers() {
    crate::runtime::eval::run_eval(|| {
        let source = "enum Boxed { Payload(Any), Other(Any) }\npure boxed(value) -> Boxed { Payload(value) }\npure other(value) -> Boxed { Other(value) }\nmatch boxed(7) { Payload(value) => print $value; _ => print \"wrong integer\" }\nmatch boxed(\"text\") { Payload(value) => print $value; _ => print \"wrong text\" }\n";
        for recursive in [false, true] {
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("scoped-tag-constructor.xsh", source);
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let (origin, application) = checked.solved.constructor_applications.iter().find(|(_, application)| match application.authority {
                    ConstructorAuthority::Nominal(crate::sema::check::QualifiedNominalIdentity::Source { member: Some(member), .. }) => member == Name::intern("Payload"), _ => false,
                }).unwrap();
                let caller = application.caller.unwrap();
                let declaration = checked.solved.declarations.get(&caller).unwrap();
                let scheme = checked.solved.graph.scheme(declaration.scheme).unwrap();
                assert!(!scheme.quantifiers.is_empty(), "the original payload parameter genuinely quantifies its declaring owner");
                assert_eq!(checked.solved.expression_owners.get(origin), Some(&caller));
                assert_eq!(checked.solved.expression_scope(*origin, Some(caller)).unwrap(), Some(declaration.scheme));
                assert!(scheme.requirement_origins.contains(&application.requirement.unwrap()), "the constructor requirement belongs to the original quantified owner");
                assert!(graph_ground_type(&checked.solved.graph, application.supplied[0].actual).is_err(), "the original supplied value retains the quantified parameter, before either concrete caller arrives");
            });
            let counters = checked.solved.graph.counters().clone();
            let solved = Arc::downgrade(&checked.solved);
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
            let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
            assert_eq!(checked.solved.graph.counters(), &counters);
            drop(parsed); drop(checked);
            assert!(solved.upgrade().is_none());
            let symbols = evaluator.indexed_program.as_ref().unwrap().symbol_owner().clone();
            let execute = || symbols.with_current(|| evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("prepared indexed program remains installed")));
            let output = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute) } else { execute() };
            assert_eq!(output.status, 0, "{:?}; {:?}", output.diagnostics, output.traceback);
            assert_eq!(output.stdout, b"7\ntext\n");
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        }
    });
}

#[test]
fn scoped_tag_constructor_refuses_foreign_frames_members_payloads_and_joint_scope_changes() {
    crate::runtime::eval::run_eval(|| {
        let source = "enum Boxed { Payload(Any), Other(Any) }\npure boxed(value, unused) -> Boxed { let observed = unused; Payload(value) }\npure other(value, unused) -> Boxed { let observed = unused; Other(value) }\nlet integer = boxed(7, false)\nlet text = boxed(\"text\", 9)\nlet distinct = other(8, true)\n";
        let program = fixture(source);
        let _symbols = program.symbol_owner().enter();
        let generic = program.store.generic.as_deref().unwrap();
        let original = generic.tag_constructors().find(|source| source.original.member == Name::intern("Payload")).unwrap().clone();
        let other = generic.tag_constructors().find(|source| source.original.member == Name::intern("Other")).unwrap().clone();
        let scope_id = original.scope.unwrap();
        let scope = generic.scope(scope_id).unwrap();
        assert_eq!(original.actuals.as_ref(), &[scope.parameters[0]]);
        assert!(matches!(original.actuals[0], TypeRef::Rigid(_)));
        assert_ne!(scope.parameters[0], scope.parameters[1]);
        let requirement = original.requirement.unwrap() as usize;
        let Requirement::TagConstructor(class) = &scope.requirements[requirement] else { panic!("the original constructor requirement remains explicit"); };
        assert_eq!(class.nominal, original.original.authority);
        assert_eq!(class.arguments.as_ref(), original.actuals.as_ref());
        let instances = generic.instances().filter(|(_, instance)| instance.scope == scope_id).collect::<Vec<_>>();
        assert_eq!(instances.len(), 2);
        for (instance_id, instance) in &instances {
            assert_eq!(instance.requirements[requirement], RequirementWitness::TagConstructor);
            generic.verify_tag_constructor_frame(original.instruction, Some(*instance_id)).unwrap();
        }
        assert_ne!(instances[0].1.parameter_types[0], instances[1].1.parameter_types[0]);
        let foreign_frame = generic.instances().find(|(_, instance)| instance.scope == other.scope.unwrap()).unwrap().0;
        assert!(generic.verify_tag_constructor_frame(original.instruction, Some(foreign_frame)).is_err());
        assert!(generic.verify_tag_constructor_frame(original.instruction, None).is_err());
        let foreign = fixture(source);
        assert!(foreign.store.generic.as_deref().unwrap().verify_tag_constructor_frame(original.instruction, Some(instances[0].0)).is_err());

        let mut changed_member = program.clone();
        let row = changed_member.store.data[original.instruction as usize].range().start as usize;
        changed_member.store.extra[row + 1] = other.payload[1];
        assert!(FullVerifier::verify(&changed_member).is_err(), "equal declared Any slots cannot substitute another nominal member");

        let unused = program.store.function_instruction_range(scope.owner.index()).unwrap().find(|&instruction| {
            program.store.tags[instruction] == FullTag::ExprParam && program.store.payload(program.store.data[instruction].range()).unwrap() == [1]
        }).unwrap();
        let mut changed_payload = program.clone();
        let block = IrBlockId::from_raw(original.field_block.0).unwrap();
        let row = changed_payload.store.blocks[block.index()].instructions.start as usize;
        changed_payload.store.extra[row + 1] = unused as u32;
        assert!(FullVerifier::verify(&changed_payload).is_err(), "the independently quantified second binder cannot supply the original first payload");

        let mut joint = program.clone();
        let receipt = joint.store.generic.as_deref_mut().unwrap().test_tag_constructor_mut(original.instruction).unwrap();
        receipt.scope = other.scope; receipt.owner = other.owner; receipt.requirement = other.requirement;
        receipt.actuals = other.actuals.clone(); receipt.original.authority = other.original.authority;
        assert!(FullVerifier::verify(&joint).is_err(), "coupled public metadata cannot replace the sealed original quantified source");

        let mut changed_class = program.clone();
        let replacement = generic.scope(other.scope.unwrap()).unwrap().requirements[other.requirement.unwrap() as usize].clone();
        changed_class.store.generic.as_deref_mut().unwrap().test_scope_mut(scope_id).unwrap().requirements[requirement] = replacement;
        assert!(FullVerifier::verify(&changed_class).is_err(), "a same-signature nominal requirement cannot replace the original constructor class");
        assert!(changed_class.store.generic.as_deref().unwrap().verify_tag_constructor_frame(original.instruction, Some(instances[0].0)).is_err(), "creation refuses changed scoped authority before its payload");
    });
}

#[test]
fn original_tag_constructor_import_aliases_do_not_require_legacy_payload_or_wire_declarations() {
    crate::runtime::eval::run_eval(|| {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("xsh-original-tag-aliases-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("model.xsh"), "##! Original nominal members.\n## Payload and empty choice.\nexport enum Choice { Payload(Str), Other(Str), Empty }\n## Original wire state.\nexport enum State: Str { Seen = \"seen\", Missing = \"missing\" }\n").unwrap();
        let source = "use model as first\nuse model as second\npure payload(value: Str) -> first.Choice { second.Payload(value) }\npure singleton() -> first.Choice { second.Empty }\npure wired() -> first.State { second.Seen }\nlet made = payload(\"original\")\nlet empty = singleton()\nlet state = wired()\n";
        let entry = root.join("entry.xsh");
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(entry.to_str().unwrap(),
            crate::loader::entry_source_from_text(entry.to_str().unwrap(), source.to_owned()), Vec::new());
        std::fs::remove_dir_all(&root).unwrap();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = SourceMap::files(&sources).first().unwrap().id();
        let mut declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let counters = bodies.solved.graph.counters().clone();
        let solved = Arc::downgrade(&bodies.solved);
        let _symbols = parsed.arena.symbol_owner().enter();
        assert_eq!(bodies.solved.constructor_applications.len(), 3);
        for application in bodies.solved.constructor_applications.values() {
            let ConstructorAuthority::Nominal(authority @ crate::sema::check::QualifiedNominalIdentity::Source { source: owner, namespace: Some(_), member: Some(_), .. }) = application.authority else { panic!("the checked imported constructor retains its declaration namespace"); };
            assert_ne!(owner, source_id);
            assert!(bodies.solved.checked_nominal_member(authority).is_ok());
            assert!(application.requirement.is_some());
        }
        assert!(!declarations.qualified_tag_variants.is_empty());
        declarations.qualified_tag_variants.clear();
        declarations.tag_variants_by_name.clear();
        declarations.wire_enums.mappings.clear();
        let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
        assert_eq!(bodies.solved.graph.counters(), &counters);
        drop(parsed); drop(declarations); drop(bodies);
        assert!(solved.upgrade().is_none());
        FullVerifier::verify(&program).unwrap();
        let generic = program.store.generic.as_deref().unwrap();
        assert_eq!(generic.tag_constructors().count(), 3);
        let payload = generic.tag_constructors().find(|source| source.original.member == Name::intern("Payload")).unwrap();
        assert_eq!(payload.original.parameters.as_ref(), &[Type::Str]);
        let empty = generic.tag_constructors().find(|source| source.original.member == Name::intern("Empty")).unwrap();
        assert!(empty.original.parameters.is_empty());
        let wire = generic.tag_constructors().find(|source| source.original.member == Name::intern("Seen")).unwrap();
        assert!(Arc::ptr_eq(wire.original.wire.as_ref().unwrap(), &program.store.wire_enums[0]));
    });
}

#[test]
fn original_tag_constructor_missing_expression_application_cannot_use_legacy_declarations() {
    crate::runtime::eval::run_eval(|| {
        for source in [
            "enum Choice { Payload(Str) }\npure selected(value: Str) -> Choice { Payload(value) }\n",
            "enum Choice { Empty }\npure selected() -> Choice { let chosen = Empty; chosen }\n",
        ] {
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                "missing-original-tag-application.xsh", crate::loader::entry_source_from_text("missing-original-tag-application.xsh", source.to_owned()), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let source_id = SourceMap::files(&sources).first().unwrap().id();
            let mut declarations = Checker::check_compact_declarations(&parsed.arena);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
            assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
            assert_eq!(bodies.solved.constructor_applications.len(), 1);
            drop(bodies);
            Arc::get_mut(&mut declarations.solved).expect("the genuine checked facts have one frontend owner").constructor_applications.clear();
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).is_err(),
                "missing original payload or zero-field application must refuse even when legacy declarations remain available: {source}");
        }
    });
}
