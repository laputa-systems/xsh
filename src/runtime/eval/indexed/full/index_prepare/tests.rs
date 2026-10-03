use super::*;
use crate::sema::check::Checker;

fn fixture(source: &str) -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "original-list-index.xsh", crate::loader::entry_source_from_text("original-list-index.xsh", source.to_owned()), Vec::new());
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

const SOURCE: &str = "pure selected(values: List[Int], position: Int, other: Int) -> Int { let _ = other; values[position] }\npure checked(values: List[Result[Int]], position: Int) -> Result[Int] { values[position] }\n";

#[test]
fn original_map_index_preserves_key_domains_and_refuses_coforged_operand_authority_after_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let source = "pure numeric(values: Map[Int, Int], key: Int, other: Int) -> Int { let _ = other; values[key] }\npure textual(values: Map[Str, Result[Int]], key: Str) -> Result[Int] { values[key] }\npure unsigned(values: Map[UInt, Int], key: UInt) -> Int { values[key] }\n";
        let program = fixture(source);
        FullVerifier::verify(&program).unwrap();
        let generic = program.store.generic.as_deref().unwrap();
        assert_eq!(generic.original_indices().count(), 3);
        let unsigned = generic.original_indices().find(|original| original.uint_key_validation.is_some()).unwrap();
        let mut missing_validation = program.clone();
        missing_validation.store.generic.as_deref_mut().unwrap().test_original_index_mut(unsigned.instruction).unwrap().uint_key_validation = None;
        assert!(FullVerifier::verify(&missing_validation).is_err());
        let mut changed_validation = program.clone();
        let validation = unsigned.uint_key_validation.as_ref().unwrap().0;
        let raw = changed_validation.store.data[validation as usize].range().start as usize;
        changed_validation.store.extra[raw] = unsigned.base;
        assert!(FullVerifier::verify(&changed_validation).is_err(), "unsigned validation cannot accept another original material source");
        let original = generic.original_indices().next().unwrap().clone();
        let mut missing = program.clone();
        missing.store.generic.as_deref_mut().unwrap().test_remove_original_indices();
        assert!(FullVerifier::verify(&missing).is_err());
        let other = fixture(source);
        let mut foreign = program.clone();
        foreign.store.generic.as_deref_mut().unwrap().test_replace_original_indices(other.store.generic.as_deref().unwrap());
        assert!(FullVerifier::verify(&foreign).unwrap_err().message.contains("foreign program"));
        let mut coforged = program.clone();
        let raw = coforged.store.data[original.index as usize].range().start as usize;
        coforged.store.extra[raw] = 2;
        coforged.store.generic.as_deref_mut().unwrap().test_original_index_mut(original.instruction).unwrap().index_parameter.as_mut().unwrap().1 = 2;
        assert!(FullVerifier::verify(&coforged).is_err(), "matching forged read and parameter receipts cannot replace the original key port");
        let operation = generic.operation(original.operation).unwrap();
        let mut semantic = program.store.semantic.clone();
        let mut builder = super::super::super::semantic::SemanticPoolBuilder::default();
        let uint = builder.intern_type(&mut semantic, &Type::UInt).unwrap();
        let mut changed_domain = operation.clone();
        changed_domain.arguments[1] = Some(TypeRef::Ground(uint));
        assert!(GenericEvidenceStore::verify_index_operation_contract(&semantic, &changed_domain).is_err(), "UInt cannot replace an original Int map key domain");
    });
}

#[test]
fn original_list_index_preserves_ground_item_and_result_carriers_after_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture(SOURCE);
        let generic = program.store.generic.as_deref().unwrap();
        assert_eq!(generic.original_indices().count(), 2);
        for original in generic.original_indices() {
            let operation = generic.operation(original.operation).unwrap();
            let [Some(TypeRef::Ground(list)), Some(TypeRef::Ground(key))] = operation.arguments.as_ref() else { panic!("original ground operands are required"); };
            let TypeRef::Ground(result) = operation.result else { panic!("original ground result is required"); };
            assert_eq!(program.store.semantic.type_children(*list).unwrap(), Some((result, None)));
            assert_eq!(program.store.semantic.type_tag(*key).unwrap(), super::super::super::semantic::TypeTag::Int);
        }
        let operation = generic.operation(generic.original_indices().next().unwrap().operation).unwrap();
        for item in [Type::Stream(Box::new(Type::Int)), Type::ProcessHandle, Type::NetJob, Type::FsRoot,
            Type::Optional(Box::new(Type::Stream(Box::new(Type::Int))))] {
            let mut semantic = program.store.semantic.clone();
            let mut builder = super::super::super::semantic::SemanticPoolBuilder::default();
            let result = builder.intern_type(&mut semantic, &item).unwrap();
            let list = builder.intern_type(&mut semantic, &Type::List(Box::new(item))).unwrap();
            let mut unsupported = operation.clone();
            unsupported.arguments[0] = Some(TypeRef::Ground(list));
            unsupported.result = TypeRef::Ground(result);
            assert!(GenericEvidenceStore::verify_index_operation_contract(&semantic, &unsupported).unwrap_err().message.contains("ownership contract"));
        }
        FullVerifier::verify(&program).unwrap();
    });
}

#[test]
fn original_list_index_refuses_missing_foreign_and_rewritten_operand_authority() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture(SOURCE);
        let generic = program.store.generic.as_deref().unwrap();
        let original = generic.original_indices().next().unwrap().clone();
        let mut missing = program.clone();
        missing.store.generic.as_deref_mut().unwrap().test_remove_original_indices();
        assert!(FullVerifier::verify(&missing).is_err());
        let other = fixture(SOURCE);
        let mut foreign = program.clone();
        foreign.store.generic.as_deref_mut().unwrap().test_replace_original_indices(other.store.generic.as_deref().unwrap());
        assert!(FullVerifier::verify(&foreign).unwrap_err().message.contains("foreign program"));
        for mutation in 0..4 {
            let mut changed = program.clone();
            let original = changed.store.generic.as_deref_mut().unwrap().test_original_index_mut(original.instruction).unwrap();
            match mutation {
                0 => original.base = original.index,
                1 => original.index_origin = original.base_origin,
                2 => original.owner = InstructionOwner::Driver(u32::MAX),
                _ => original.operation = other.store.generic.as_deref().unwrap().original_indices().next().unwrap().operation,
            }
            assert!(FullVerifier::verify(&changed).is_err());
        }
        for operand in [0, 1] {
            let mut changed = program.clone();
            let raw = changed.store.data[original.instruction as usize].range().start as usize;
            changed.store.extra[raw + operand] = if operand == 0 { original.index } else { original.base };
            assert!(FullVerifier::verify(&changed).is_err());
        }
        let mut changed_read = program.clone();
        let raw = changed_read.store.data[original.index as usize].range().start as usize;
        changed_read.store.extra[raw] = 2;
        assert!(FullVerifier::verify(&changed_read).is_err(), "a different Int parameter cannot replace the original key read");
        let mut changed_result = program.clone();
        let evidence = changed_result.store.generic.as_deref_mut().unwrap();
        let operation = evidence.test_operation_mut(original.operation).unwrap();
        operation.result = operation.arguments[0].unwrap();
        assert!(FullVerifier::verify(&changed_result).is_err());
    });
}

#[test]
fn original_list_index_executes_both_routes_with_frontend_disposed_in_the_worker() {
    crate::runtime::eval::run_eval(|| {
        let source = "type Config = {workers: Int}\nconst field = \"workers\"\npure selected(values: List[Int], position: Int) -> Int { values[position] }\npure checked(values: List[Result[Int]], position: Int) -> Result[Int] { values[position] }\npure unsigned(values: List[UInt], position: UInt) -> UInt { values[position] }\npure record_count(config: Config) -> Int { config[field] }\nprint ${selected([4, 7], 1)} ${checked([Ok(3)], 0) is Ok(3)} ${unsigned([8], 0)} ${record_count({workers: 4})}\n";
        for recursive in [false, true] {
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("original-index-execution.xsh", source);
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
            assert_eq!(output.stdout, b"7 true 8 4\n");
            assert!(output.stderr.is_empty());
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        }
    });
}

#[test]
fn original_record_index_preserves_constant_field_and_refuses_changed_layout_after_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let source = "type Config = {workers: Int, retries: Int}\nconst field = \"workers\"\npure selected(config: Config) -> Int { config[field] }\n";
        let program = fixture(source);
        program.symbol_owner().with_current(|| {
            FullVerifier::verify(&program).unwrap();
            let original = program.store.generic.as_deref().unwrap().original_indices().next().unwrap().clone();
            let (field, layout, slot) = original.projection.expect("constant record index retains its selected physical field");
            assert_eq!(field, Name::intern("workers"));
            assert_eq!(program.store.generic.as_deref().unwrap().layout(layout).unwrap().fields[slot as usize].0, field);
            for mutation in 0..3 {
                let mut changed = program.clone();
                let original = changed.store.generic.as_deref_mut().unwrap().test_original_index_mut(original.instruction).unwrap();
                match mutation {
                    0 => original.projection = None,
                    1 => original.projection.as_mut().unwrap().0 = Name::intern("retries"),
                    _ => original.projection.as_mut().unwrap().2 = slot.wrapping_add(1),
                }
                assert!(FullVerifier::verify(&changed).is_err());
            }
            let mut changed_key = program.clone();
            let raw = changed_key.store.data[original.index_material as usize].range().start as usize;
            changed_key.store.extra[raw] = u32::MAX;
            assert!(FullVerifier::verify(&changed_key).is_err());
        });
    });
}

#[test]
fn original_index_retains_checked_uint_operand_lineage_after_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let source = "pure list_return(n: Int) -> List[UInt] { return [n] }\npure selected(n: Int) -> Int { return list_return(n)[0] }\npure checked(values: Map[UInt, Int], position: UInt) -> Int { values[position] }\n";
        let program = fixture(source);
        FullVerifier::verify(&program).unwrap();
        let generic = program.store.generic.as_deref().unwrap();
        assert_eq!(generic.original_indices().count(), 2);
        assert!(generic.original_indices().any(|original| !original.base_wrappers.is_empty() || !original.index_wrappers.is_empty()), "the source must execute an actual checked operand wrapper");
        for original in generic.original_indices() {
            for wrapper in original.base_wrappers.iter().chain(original.index_wrappers.iter()) {
                let mut changed = program.clone();
                let raw = changed.store.data[wrapper.instruction as usize].range().start as usize;
                changed.store.extra[raw] = original.base;
                assert!(FullVerifier::verify(&changed).is_err(), "checked index operands retain their exact original wrapper child");
            }
        }
    });
}

#[test]
fn original_result_index_preserves_dns_record_carrier_and_refuses_rewritten_source() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture("proc selected() [net, error] -> Str { dns.lookup(\"example.test\")?[0].value }\n");
        program.symbol_owner().with_current(|| {
            FullVerifier::verify(&program).unwrap();
            let original = program.store.generic.as_deref().unwrap().original_indices().next().unwrap().clone();
            assert_eq!(program.store.tags[original.base as usize], FullTag::ExprTry);
            assert!(program.store.generic.as_deref().unwrap().registered_instruction_origin(original.base, false).is_none());
            assert_eq!(program.store.tags[original.base_material as usize], FullTag::ExprModuleCall);
            let postfix = original.postfix_base.as_ref().expect("guarded index retains its independent Result carrier authority");
            assert_eq!(postfix.source_instruction, original.base_material);
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_original_index_mut(original.instruction).unwrap().postfix_base = None;
            assert!(FullVerifier::verify(&missing).is_err());
            let mut changed = program.clone();
            let raw = changed.store.data[original.base as usize].range().start as usize;
            changed.store.extra[raw] = original.index;
            changed.store.generic.as_deref_mut().unwrap().test_original_index_mut(original.instruction).unwrap().base_material = original.index;
            assert!(FullVerifier::verify(&changed).is_err(), "agreeing rewritten carrier and index receipt cannot replace the authored DNS source");
        });
    });
}

#[test]
fn original_result_index_executes_dns_schema_success_and_error_on_both_workers_after_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(fixture("pure selected(values: Result[List[DnsLookup]]) -> Result[Str] { Ok(values?[0].value) }\n"));
        program.symbol_owner().with_current(|| {
            let function = LoweredFunctionKey::Name(Name::intern("selected"));
            let row = crate::runtime::value::Value::Record(crate::runtime::value::RecordMap::from([
                (Arc::from("name"), crate::runtime::value::Value::Str(Arc::from("example.test"))),
                (Arc::from("record"), crate::runtime::value::Value::Str(Arc::from("A"))),
                (Arc::from("value"), crate::runtime::value::Value::Str(Arc::from("127.0.0.1"))),
                (Arc::from("ttl"), crate::runtime::value::Value::Int(60)),
            ]));
            for recursive in [false, true] {
                for accepted in [false, true] {
                    let argument = if accepted {
                        crate::runtime::value::Value::ok(crate::runtime::value::Value::List(vec![row.clone()]))
                    } else {
                        crate::runtime::value::Value::err(crate::runtime::value::error_constructor("dns-name", "rejected"))
                    };
                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure, &[argument],
                        Span::new(program.store.source_id, 0, 0)).expect("selected DNS schema function remains installed");
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call).unwrap();
                    if accepted { assert_eq!(result, crate::runtime::value::Value::ok(crate::runtime::value::Value::Str(Arc::from("127.0.0.1")))); }
                    else { assert_eq!(result, crate::runtime::value::Value::err(crate::runtime::value::error_constructor("dns-name", "rejected"))); }
                }
            }
        });
    });
}
