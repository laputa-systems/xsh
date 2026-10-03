use super::*;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::Value;
use crate::source::SourceMap;
use crate::syntax::parser::Parser;
use crate::sema::check::Checker;

fn cli_program(source: &str) -> Arc<FullProgram> {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("cli-proof.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let symbols = parsed.arena.symbol_owner().clone();
    symbols.with_current(|| {
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let solved = Arc::downgrade(&checked.solved);
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
        evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
        let program = Arc::clone(evaluator.indexed_program.as_ref().unwrap());
        drop(checked);
        drop(parsed);
        assert!(solved.upgrade().is_none(), "the prepared CLI call must dispose of inference authority");
        program
    })
}

#[test]
fn cli_descriptors_retain_exact_plan_result_and_recipes_after_frontend_disposal() {
    crate::runtime::eval::run_eval(|| {
        let source = include_str!("../../../../../../tests/fixtures/frontend-indexed/cli-constant-descriptors.xsh");
        let program = cli_program(source);
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| proof.contract.cli_descriptor.is_some()).collect::<Vec<_>>();
            assert_eq!(calls.len(), 3);
            for (_, proof) in calls {
                let descriptor = proof.contract.cli_descriptor.as_ref().unwrap();
                assert!(program.store.prepared_cli_plans.iter().any(|plan| Arc::ptr_eq(plan, &descriptor.plan)));
                assert!(proof.contract.verify_cli_descriptor(&program.store.semantic).unwrap());
                assert_eq!(proof.contract.arguments.len(), 2);
                assert_eq!(proof.contract.binding.supplied_slots.as_ref(), &[0, 1]);
            }
            FullVerifier::verify(&program).unwrap();
        });
        for recursive in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let name = program.symbol_owner().with_current(|| Name::intern("descriptor_values"));
            let execute = || {
                assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                evaluator.call_indexed_direct(LoweredFunctionKey::Name(name), LoweredFunctionKind::Proc, &[], Span::new(program.store.source_id, 0, 0)).unwrap().unwrap()
            };
            let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(name), recursive, execute);
            assert_eq!(result, Value::ok(Value::Str(Arc::from("6/4/3"))));
        }
    });
}

#[test]
fn cli_descriptors_reject_missing_foreign_rewritten_plan_result_and_recipe() {
    crate::runtime::eval::run_eval(|| {
        let source = include_str!("../../../../../../tests/fixtures/frontend-indexed/cli-constant-descriptors.xsh");
        let program = cli_program(source);
        let foreign = cli_program(source);
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_native_calls().find(|(_, proof)| proof.contract.cli_descriptor.is_some()).unwrap();
            let source = generic.native_call_source(proof.source).unwrap();
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
            let mut other = program.store.clone();
            other.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().source = foreign.generic_evidence().unwrap().ground_native_calls().next().unwrap().1.source;
            assert!(FullVerifier::verify_generic_evidence(&other).is_err());
            let range = program.store.data[source.instruction as usize].range();
            let mut erased = program.store.clone();
            erased.extra[range.start as usize + 1] = 0;
            assert!(FullVerifier::verify_generic_evidence(&erased).is_err());
            let mut changed = program.store.clone();
            changed.prepared_cli_plans[0] = Arc::clone(&foreign.store.prepared_cli_plans[0]);
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "a separately checked identical descriptor is foreign authority");
            let mut jointly_changed = program.store.clone();
            let replacement = Arc::clone(&foreign.store.prepared_cli_plans[0]);
            jointly_changed.prepared_cli_plans[0] = Arc::clone(&replacement);
            jointly_changed.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.cli_descriptor.as_mut().unwrap().plan = Arc::clone(&replacement);
            jointly_changed.generic.as_deref_mut().unwrap().test_native_call_source_mut(proof.source).unwrap().expected.cli_descriptor.as_mut().unwrap().plan = replacement;
            assert!(FullVerifier::verify_generic_evidence(&jointly_changed).is_err(), "rewriting the plan pool and both public receipts does not rewrite original checked authority");
            let descriptor = proof.contract.cli_descriptor.as_ref().unwrap();
            let mut default = program.store.clone();
            if let Some(integer) = descriptor.rows.iter().find(|row| row.tag == FullTag::ExprInt as u16) {
                let value = default.data[integer.instruction as usize].range();
                default.extra[value.start as usize] ^= 1;
            } else {
                let constant = descriptor.rows.iter().find(|row| row.constant.is_some()).unwrap();
                default.prepared_constants[constant.payload[0] as usize].0 = LoweredValue::Unit;
            }
            assert!(FullVerifier::verify_generic_evidence(&default).is_err(), "a descriptor default keeps its original checked constant value");
            let mut result = program.store.clone();
            result.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.result = proof.contract.arguments[0].ty;
            assert!(FullVerifier::verify_generic_evidence(&result).is_err());
            assert!(FullVerifier::native_call_result(&result, result.generic.as_deref().unwrap(), source.instruction, source.owner).is_err(), "runtime dispatch refuses the rewritten parsed carrier before evaluating operands");
            let mut recipe = program.store.clone();
            recipe.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.arguments[0].original.name = Some(Name::intern("schema"));
            assert!(FullVerifier::verify_generic_evidence(&recipe).is_err());
            assert!(FullVerifier::native_call_result(&recipe, recipe.generic.as_deref().unwrap(), source.instruction, source.owner).is_err(), "runtime dispatch preserves the original argument recipe");
            let mut both = program.store.clone();
            let replacement = proof.contract.arguments[0].ty;
            both.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.result = replacement;
            both.generic.as_deref_mut().unwrap().test_native_call_source_mut(proof.source).unwrap().expected.result = replacement;
            assert!(FullVerifier::verify_generic_evidence(&both).is_err(), "agreeing public copies cannot replace the sealed original contract");
        });
    });
}

#[test]
fn cli_parsed_result_layout_preserves_extra_fields_and_error_payloads() {
    crate::runtime::eval::run_eval(|| {
        let source = include_str!("../../../../../../tests/fixtures/frontend-indexed/cli-constant-descriptors.xsh");
        let program = cli_program(source);
        program.symbol_owner().with_current(|| {
            let (_, proof) = program.generic_evidence().unwrap().ground_native_calls().find(|(_, proof)| {
                matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::CliParse, .. })
            }).unwrap();
            let descriptor = proof.contract.cli_descriptor.as_ref().unwrap();
            let evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            let span = Span::new(program.store.source_id, 0, 0);
            let fields = BTreeMap::from([
                (Arc::from("jobs"), LoweredValue::Int(7)),
                (Arc::from("additional"), LoweredValue::Str(Arc::from("retained"))),
            ]);
            let value = descriptor.materialize_result(&evaluator, LoweredValue::ResultOk(Box::new(LoweredValue::Record(Arc::new(fields)))), span).unwrap();
            let LoweredValue::ResultOk(value) = value else { panic!("parsed Result success"); };
            let LoweredValue::RecordVec(fields) = *value else { panic!("numeric parsed record layout"); };
            assert_eq!(fields[0], (Name::intern("jobs"), LoweredValue::Int(7)));
            assert!(fields.iter().any(|(name, value)| *name == Name::intern("additional") && *value == LoweredValue::Str(Arc::from("retained"))));
            let error = LoweredValue::ResultErr(Box::new(Value::Str(Arc::from("parse-error"))));
            assert_eq!(descriptor.materialize_result(&evaluator, error.clone(), span).unwrap(), error);
            assert!(descriptor.materialize_result(&evaluator, LoweredValue::Unit, span).is_err());
            let wrong = LoweredValue::ResultOk(Box::new(LoweredValue::Record(Arc::new(BTreeMap::from([(Arc::from("jobs"), LoweredValue::Str(Arc::from("wrong")))])))));
            assert!(descriptor.materialize_result(&evaluator, wrong, span).is_err());
        });
    });
}

#[test]
fn prepared_constant_validation_preserves_checked_containers_and_nominal_owners() {
    crate::symbol::SymbolOwner::new().with_current(|| {
        let matches = |value: &LoweredValue, ty: &Type| prepared_constant_matches_type(value, ty, 0, &mut 0).unwrap();
        let field = Name::intern("jobs");
        let extra = Name::intern("extra");
        let record = Type::Record(BTreeMap::from([(field, Type::UInt)]));
        let values = LoweredValue::RecordVec(Arc::new(vec![(extra, LoweredValue::Str(Arc::from("retained"))), (field, LoweredValue::Int(3))]));
        assert!(matches(&values, &record));
        assert!(!matches(&LoweredValue::RecordVec(Arc::new(vec![(extra, LoweredValue::Int(3))])), &record));
        assert!(!matches(&LoweredValue::RecordVec(Arc::new(vec![(field, LoweredValue::Int(3)), (field, LoweredValue::Int(4))])), &record));
        assert!(!matches(&LoweredValue::RecordVec(Arc::new(vec![(field, LoweredValue::Int(-1))])), &record));
        let nondata = LoweredValue::RecordVec(Arc::new(vec![(field, LoweredValue::Int(3)), (extra, LoweredValue::Unit)]));
        assert!(!matches(&nondata, &record));
        let uint_map = Type::Map(Box::new(Type::UInt), Box::new(Type::List(Box::new(Type::Str))));
        let map = |key| LoweredValue::Map(Arc::new(BTreeMap::from([(crate::map_key::MapKey::Int(key), LoweredValue::List(vec![LoweredValue::Str(Arc::from("data"))]))])));
        assert!(matches(&map(2), &uint_map));
        assert!(!matches(&map(-2), &uint_map));
        assert!(!matches(&map(2), &Type::Map(Box::new(Type::Str), Box::new(Type::List(Box::new(Type::Str))))));
        let owner = Name::intern("workers.Mode");
        let variant = Name::intern("Fast");
        let wire = Arc::new(crate::sema::wire_enums::WireEnumMapping { type_name: owner, variants: BTreeMap::from([(variant, Arc::from("fast"))]) });
        let tag = LoweredValue::Tag(Box::new(crate::runtime::eval::LoweredTagValue { type_name: owner, name: Arc::from("Fast"), fields: Vec::new(), wire: Some(wire) }));
        assert!(matches(&tag, &Type::Tag(owner)));
        assert!(!matches(&tag, &Type::Tag(Name::intern("other.Mode"))));
        let mut wrong_wire = tag.clone();
        let LoweredValue::Tag(value) = &mut wrong_wire else { unreachable!() };
        Arc::make_mut(value.wire.as_mut().unwrap()).type_name = Name::intern("other.Mode");
        assert!(!matches(&wrong_wire, &Type::Tag(owner)));
        let mut work = 65536;
        assert!(prepared_constant_matches_type(&LoweredValue::Int(1), &Type::Int, 0, &mut work).is_err());
        assert!(prepared_constant_matches_type(&LoweredValue::Int(1), &Type::Int, 256, &mut 0).is_err());
    });
}

#[test]
fn cli_inferred_defaults_preserve_field_types_precedence_and_policy_after_frontend_disposal() {
    crate::runtime::eval::run_eval(|| {
        let source = include_str!("../../../../../../tests/fixtures/frontend-indexed/cli-inferred-descriptor-defaults.xsh");
        let program = cli_program(source);
        program.symbol_owner().with_current(|| {
            let fields = BTreeMap::from([
                (Name::intern("jobs"), Type::Int),
                (Name::intern("optional"), Type::Optional(Box::new(Type::Str))),
                (Name::intern("tag"), Type::List(Box::new(Type::Str))),
                (Name::intern("verbose"), Type::Bool),
            ]);
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| proof.contract.cli_descriptor.is_some()).collect::<Vec<_>>();
            assert_eq!(calls.len(), 6);
            for (_, proof) in calls {
                let descriptor = proof.contract.cli_descriptor.as_ref().unwrap();
                assert_eq!(descriptor.plan.values_type(), Type::Record(fields.clone()));
                assert!(proof.contract.verify_cli_descriptor(&program.store.semantic).unwrap());
            }
            FullVerifier::verify(&program).unwrap();
        });
        for recursive in [false, true] {
            for name in ["inferred_descriptor_values", "inferred_descriptor_duplicate", "inferred_descriptor_absent"] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let execute = || {
                    assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                    evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &[], Span::new(program.store.source_id, 0, 0)).unwrap().unwrap()
                };
                let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, execute);
                if name == "inferred_descriptor_values" {
                    assert_eq!(result, Value::ok(Value::Str(Arc::from("4/8/9/3/0/false/default/env/argv"))));
                } else if name == "inferred_descriptor_absent" {
                    assert_eq!(result, Value::ok(Value::Null));
                } else {
                    let Value::Result(crate::runtime::value::ResultValue::Err(error)) = result else { panic!("strict scalar duplicate remains a usage error"); };
                    assert_eq!(error.error_kind(), Some("cli-parse"));
                }
            }
        }
    });
}

#[test]
fn cli_inferred_defaults_reject_jointly_rewritten_constant_and_public_receipts() {
    crate::runtime::eval::run_eval(|| {
        let source = include_str!("../../../../../../tests/fixtures/frontend-indexed/cli-inferred-descriptor-defaults.xsh");
        let program = cli_program(source);
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_native_calls().find(|(_, proof)| proof.contract.cli_descriptor.is_some()).unwrap();
            let original = generic.native_call_source(proof.source).unwrap();
            let descriptor = proof.contract.cli_descriptor.as_ref().unwrap();
            let row = descriptor.rows.iter().find(|row| row.constant.is_some()).unwrap();
            let mut changed = program.store.clone();
            let mut value = row.constant.clone().unwrap();
            fn field<'a>(value: &'a mut LoweredValue, name: Name) -> &'a mut LoweredValue {
                match value {
                    LoweredValue::Record(values) => Arc::make_mut(values).get_mut(name.as_str().as_str()).unwrap(),
                    LoweredValue::RecordVec(values) => &mut Arc::make_mut(values).iter_mut().find(|(field, _)| *field == name).unwrap().1,
                    _ => panic!("descriptor retains checked record data"),
                }
            }
            *field(field(&mut value, Name::intern("jobs")), Name::intern("default")) = LoweredValue::Int(5);
            changed.prepared_constants[row.payload[0] as usize].0 = value.clone();
            let mut rows = descriptor.rows.to_vec();
            rows.iter_mut().find(|candidate| candidate.instruction == row.instruction).unwrap().constant = Some(value);
            let rows: Arc<[super::super::super::generic::CliDescriptorRow]> = rows.into();
            changed.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.cli_descriptor.as_mut().unwrap().rows = Arc::clone(&rows);
            changed.generic.as_deref_mut().unwrap().test_native_call_source_mut(proof.source).unwrap().expected.cli_descriptor.as_mut().unwrap().rows = rows;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "agreeing public snapshots cannot replace the original checked default");
            assert!(FullVerifier::native_call_result(&changed, changed.generic.as_deref().unwrap(), original.instruction, original.owner).is_err(), "rewritten defaults are refused before operand evaluation");
        });
    });
}

#[test]
fn cli_imported_command_named_spread_retains_original_recipes_after_frontend_disposal() {
    crate::runtime::eval::run_eval(|| {
        let source = include_str!("../../../../../../tests/fixtures/frontend-indexed/cli-imported-descriptor-recipes/entry.xsh");
        let entry = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/frontend-indexed/cli-imported-descriptor-recipes/entry.xsh");
        for recursive in [false, true] {
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(entry.to_str().unwrap(),
                crate::loader::entry_source_from_text(entry.to_str().unwrap(), source.to_string()), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone();
            symbols.with_current(|| {
                let checked = Checker::check_arena(&parsed.arena, source);
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let (origin, recipes) = checked.solved.argument_sources.iter().find(|(_, recipes)| recipes.iter().any(|recipe|
                    matches!(recipe.value, crate::sema::arguments::ArgumentValueSource::RecordField { .. }))).unwrap();
                let origin = *origin;
                let recipes = recipes.clone();
                let solved = Arc::downgrade(&checked.solved);
                let source_id = sources.files()[0].id();
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
                let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
                let program = Arc::clone(evaluator.indexed_program.as_ref().unwrap());
                drop(checked);
                drop(parsed);
                assert!(solved.upgrade().is_none(), "imported descriptors retain no inference authority");
                let generic = program.generic_evidence().unwrap();
                let (id, proof) = generic.ground_native_calls().find(|(_, proof)| proof.contract.cli_descriptor.is_some()).unwrap();
                let original = generic.native_call_source(proof.source).unwrap();
                assert_eq!(original.origin, origin);
                assert_eq!(proof.contract.arguments.iter().map(|argument| &argument.original).collect::<Vec<_>>(), recipes.iter().collect::<Vec<_>>());
                assert_eq!(original.record_arguments.len(), 2);
                assert_eq!(proof.contract.binding.supplied_slots.as_ref(), &[0, 1]);
                assert_eq!(proof.contract.arguments.iter().map(|argument| argument.original.name.unwrap()).collect::<Vec<_>>(), [Name::intern("argv"), Name::intern("commands")]);
                FullVerifier::verify(&program).unwrap();
                let mut changed = program.store.clone();
                changed.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.arguments[1].original.entry_index += 1;
                changed.generic.as_deref_mut().unwrap().test_native_call_source_mut(proof.source).unwrap().expected.arguments[1].original.entry_index += 1;
                assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "agreeing public recipes cannot replace the authored spread entry");
                let execute = || evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("the original imported program remains installed"));
                let output = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute) } else { execute() };
                assert_eq!(output.status, 0);
                assert_eq!(output.stdout, b"workspace extra\n");
                assert!(output.stderr.is_empty());
            });
        }
    });
}

#[test]
fn cli_dynamic_full_carrier_retains_original_result_and_operand_recipes_after_frontend_disposal() {
    crate::runtime::eval::run_eval(|| {
        let source = include_str!("../../../../../../tests/fixtures/frontend-indexed/cli-dynamic-full-carrier/entry.xsh");
        let entry = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/frontend-indexed/cli-dynamic-full-carrier/entry.xsh");
        for recursive in [false, true] {
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(entry.to_str().unwrap(),
                crate::loader::entry_source_from_text(entry.to_str().unwrap(), source.to_string()), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let symbols = parsed.arena.symbol_owner().clone();
            symbols.with_current(|| {
                let checked = Checker::check_arena(&parsed.arena, source);
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let (origin, operation) = checked.solved.operations.iter().find(|(_, operation)| {
                    let selected = checked.solved.graph.candidate_evidence(operation.requirement).unwrap().unwrap();
                    matches!(checked.solved.operation_catalog.candidate(&checked.solved.graph, selected.candidate).unwrap(),
                        crate::sema::check::SolvedOperationAuthority::Registry(metadata) if metadata.operation == RuntimeOp::CliParseFull)
                }).unwrap();
                let origin = *origin;
                let original_result = checked.solved.graph.export_type(operation.result).unwrap();
                let recipes = checked.solved.argument_sources[&origin].clone();
                assert!(!checked.solved.registry_boundaries.contains_key(&origin));
                let solved = Arc::downgrade(&checked.solved);
                let source_id = sources.files()[0].id();
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
                let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
                let program = Arc::clone(evaluator.indexed_program.as_ref().unwrap());
                drop(checked);
                drop(parsed);
                assert!(solved.upgrade().is_none(), "a dynamic CLI carrier keeps no inference authority");
                let generic = program.generic_evidence().unwrap();
                let (id, proof) = generic.ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority,
                    PreparedOperationAuthority::Registry { operation: RuntimeOp::CliParseFull, .. })).unwrap();
                let original = generic.native_call_source(proof.source).unwrap();
                assert_eq!(original.origin, origin);
                assert!(proof.contract.cli_descriptor.is_none());
                let TypeRef::Ground(result) = proof.contract.result else { panic!("canonical dynamic carrier remains closed"); };
                assert_eq!(program.store.semantic.to_type(result).unwrap(), original_result);
                assert_eq!(result, program.store.semantic.signature_return_type(proof.contract.signature).unwrap());
                assert_eq!(proof.contract.arguments.iter().map(|argument| &argument.original).collect::<Vec<_>>(), recipes.iter().collect::<Vec<_>>());
                let Type::Result(success, error) = &original_result else { panic!("original dynamic Result envelope"); };
                assert_eq!(error.as_ref(), &Type::Error);
                let Type::Record(fields) = success.as_ref() else { panic!("original dynamic full carrier fields"); };
                assert_eq!(fields[&Name::intern("values")], Type::ErasedRecord);
                assert_eq!(fields[&Name::intern("sources")], Type::ErasedRecord);
                assert_eq!(fields[&Name::intern("warnings")], Type::List(Box::new(Type::Str)));
                FullVerifier::verify(&program).unwrap();
                let mut changed = program.store.clone();
                changed.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.result = proof.contract.arguments[0].ty;
                changed.generic.as_deref_mut().unwrap().test_native_call_source_mut(proof.source).unwrap().expected.result = proof.contract.arguments[0].ty;
                assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "agreeing public carriers cannot replace the original canonical Result");
                assert!(FullVerifier::native_call_result(&changed, changed.generic.as_deref().unwrap(), original.instruction, original.owner).is_err());
                let mut changed = program.store.clone();
                changed.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.arguments[1].original = proof.contract.arguments[0].original.clone();
                changed.generic.as_deref_mut().unwrap().test_native_call_source_mut(proof.source).unwrap().expected.arguments[1].original = proof.contract.arguments[0].original.clone();
                assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "a dynamic descriptor keeps its own authored operand recipe");
                let execute = || evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("the original dynamic CLI program remains installed"));
                let output = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute) } else { execute() };
                assert_eq!(output.status, 0);
                assert_eq!(output.stdout, b"4 argv 1\n");
                assert!(output.stderr.is_empty());
            });
        }
    });
}
