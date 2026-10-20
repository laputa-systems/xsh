use super::super::*;
use crate::sema::check::Checker;

fn module_fixture(entry_source: &str, module_source: &str) -> FullProgram {
    module_graph_fixture(entry_source, &[("model", module_source)])
}

fn module_graph_fixture(entry_source: &str, modules: &[(&str, &str)]) -> FullProgram {
    let mut sources = SourceMap::new();
    let entry_id = sources.add_file("import-entry.xsh", entry_source);
    let mut builder = crate::syntax::arena::ArenaProgramBuilder::with_token_capacity(256);
    let entry = crate::syntax::parser::Parser::parse_source_into_arena_builder(entry_id, entry_source, &mut builder);
    assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
    for (key, module_source) in modules {
        let module_id = sources.add_file(format!("{key}.xsh"), *module_source);
        let model = crate::syntax::parser::Parser::parse_source_into_arena_builder(module_id, module_source, &mut builder);
        assert!(model.diagnostics.is_empty(), "{:?}", model.diagnostics);
        let name = builder.symbol_owner().with_current(|| Name::intern(key));
        builder.push_arena_module(key.to_string(), name, model.statements);
    }
    let mut resolved = 0;
    for statement in builder.statement_ids(entry.statements) {
        if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
            let key = modules[resolved].0;
            resolved += 1;
            builder.set_use_resolved(use_id, Arc::from(key));
        }
    }
    let arena = builder.finish_with_statements(entry.statements);
    let declarations = Checker::check_compact_declarations(&arena, &sources, entry_id);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    FullBuilder::build_compact(&arena, &declarations, entry_source, Arc::new(sources), entry_id).unwrap()
}

fn execute_both_routes(program: FullProgram) {
    let program = Arc::new(program);
    for force_frames in [false, true] {
        let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&program));
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::new(program.store.source_id, 0, 0)).unwrap().unwrap();
        }
    }
}

#[test]
fn imported_module_storage_executes_after_frontend_disposal() {
    let program = module_fixture(
        "use model\nvar total = 0\nproc imported() [] -> Int { total += model.marker; total }\nassert imported() == 7\n",
        "##! Module storage fixture.\n## The count.\nexport let marker: Int = 7\n",
    );
    FullVerifier::verify(&program).unwrap();
    execute_both_routes(program);
}

#[test]
fn imported_module_storage_accepts_erased_read_precision() {
    let mut program = module_fixture(
        "use model\npure observed() -> Bool { model == model }\nassert observed()\n",
        "##! Module storage fixture.\n## The count.\nexport let marker: Int = 7\n## Preserves its input.\nexport pure identity(value: Int) -> Int { value }\n",
    );
    FullVerifier::verify(&program).unwrap();
    let step = program.store.driver_steps.iter().position(|step| step.tag == FullDriverTag::Use).unwrap();
    let registered = program.store.driver_steps[step].registered_module_type;
    assert_ne!(registered, IR_NONE);
    erase_module_read_precision(&mut program, registered);
    FullVerifier::verify(&program).unwrap();
    execute_both_routes(program);
}

fn erase_module_read_precision(program: &mut FullProgram, registered: u32) {
    let dynamic = SemanticPoolBuilder::default().intern_type(&mut program.store.semantic, &Type::DynamicModule).unwrap();
    replace_module_reads(program, registered, dynamic);
}

fn replace_module_reads(program: &mut FullProgram, registered: u32, replacement: TypeId) {
    for sync in &mut program.store.driver_sync {
        if sync.type_id.raw() == registered { sync.type_id = replacement; }
    }
    for capture in &mut program.store.captures {
        if capture.type_id.raw() == registered { capture.type_id = replacement; }
    }
    for slot in &mut program.store.driver_slots {
        if slot.type_id.raw() == registered { slot.type_id = replacement; }
    }
    for ty in &mut program.store.slot_type_facts {
        if *ty == registered { *ty = replacement.raw(); }
    }
    for fact in &mut program.store.checked_expr_types {
        if fact.type_id.raw() == registered { fact.type_id = replacement; }
    }
}

#[test]
fn imported_module_storage_rejects_forged_registered_exports() {
    let mut program = module_fixture(
        "use model\nlet saved = model\nvar total = 0\nproc imported() [] -> Int { total += model.marker; total }\nassert imported() == 7\n",
        "##! Module storage fixture.\n## The count.\nexport let marker: Int = 7\n## Preserves its input.\nexport pure identity(value: Int) -> Int { value }\n",
    );
    FullVerifier::verify(&program).unwrap();
    let step = program.store.driver_steps.iter().position(|step| step.tag == FullDriverTag::Use).unwrap();
    let actual = TypeId::from_raw(program.store.driver_steps[step].registered_module_type).unwrap();
    let Type::Module(mut module) = program.store.semantic.to_type(actual).unwrap() else { panic!("registered module") };
    let _symbols = program.symbol_owner().enter();
    Arc::make_mut(&mut module).exports.insert(Name::intern("marker"), crate::sema::types::ModuleExportType::Value { ty: Type::Bool, optional: false });
    let forged = SemanticPoolBuilder::default().intern_type(&mut program.store.semantic, &Type::Module(module)).unwrap();
    replace_module_reads(&mut program, actual.raw(), forged);
    program.store.driver_steps[step].registered_module_type = forged.raw();
    let error = FullVerifier::verify(&program).unwrap_err();
    assert_eq!(error.message, "compound storage writer has incompatible operands");
}

fn module_export_binding_word(program: &FullProgram, step: usize, name: &str) -> usize {
    let _symbols = program.symbol_owner().enter();
    let payload = program.store.payload(program.store.driver_steps[step].data.range()).unwrap();
    let block = IrBlockId::from_raw(payload[4]).unwrap();
    let range = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
    let words = &program.store.extra[range.clone()];
    let mut cursor = 1;
    for _ in 0..words[0] {
        if Name::from_symbol(crate::symbol::Symbol::from_raw(words[cursor])).as_str() == name {
            assert_eq!(words[cursor + 1], 0);
            return range.start + cursor + 2;
        }
        cursor += if words[cursor + 1] == 3 { 4 } else { 3 };
    }
    panic!("module value export")
}

#[test]
fn imported_module_storage_rejects_another_childs_value_origin() {
    let program = module_graph_fixture(
        "use model\nuse other\npure observed() -> Bool { model == model }\npure second() -> Bool { other == other }\nassert observed()\nassert second()\n",
        &[
            ("model", "##! First module.\n## The count.\nexport let marker: Int = 7\n"),
            ("other", "##! Second module.\n## The count.\nexport let marker: Int = 11\n"),
        ],
    );
    FullVerifier::verify(&program).unwrap();
    let steps = program.store.driver_steps.iter().enumerate().filter_map(|(index, step)| (step.tag == FullDriverTag::Use).then_some(index)).collect::<Vec<_>>();
    assert_eq!(steps.len(), 2);
    let first = module_export_binding_word(&program, steps[0], "marker");
    let second = module_export_binding_word(&program, steps[1], "marker");
    assert_ne!(program.store.extra[first], program.store.extra[second]);
    for broad in [false, true] {
        let mut forged = program.clone();
        if broad {
            let registered = forged.store.driver_steps[steps[0]].registered_module_type;
            erase_module_read_precision(&mut forged, registered);
            forged.store.driver_steps[steps[0]].registered_module_type = IR_NONE;
            FullVerifier::verify(&forged).unwrap();
        }
        forged.store.extra[first] = forged.store.extra[second];
        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(error.message.contains("module value export identity is not an exported child binding"), "{}", error.message);
    }
}

#[test]
fn imported_module_storage_rejects_forged_member_context() {
    let mut program = module_fixture(
        "use model\nlet chosen = model\nvar total = 0\nproc imported() [] -> Int { let ignored = chosen.text; total += chosen.marker; total }\nassert imported() == 7\n",
        "##! Module storage fixture.\n## The count.\nexport let marker: Int = 7\n## The text.\nexport let text: Str = \"wrong\"\n",
    );
    FullVerifier::verify(&program).unwrap();
    let marker = (1..=program.store.strings.len() as u32).find(|raw| program.store.string(*raw).unwrap() == "marker").unwrap();
    let text = (1..=program.store.strings.len() as u32).find(|raw| program.store.string(*raw).unwrap() == "text").unwrap();
    let (instruction, field) = program.store.tags.iter().enumerate().find_map(|(instruction, tag)| {
        if *tag != FullTag::ExprField { return None; }
        let words = program.store.data[instruction].range().bounds(program.store.extra.len()).unwrap();
        (program.store.extra[words.start + 1] == marker).then_some((instruction, words.start + 1))
    }).unwrap();
    program.store.extra[field] = text;
    let owner = (0..program.store.functions.len()).find(|index| program.store.function_instruction_range(*index).unwrap().contains(&instruction)).unwrap();
    let int = SemanticPoolBuilder::default().intern_type(&mut program.store.semantic, &Type::Int).unwrap();
    program.store.checked_expr_types.retain(|fact| fact.instruction != instruction as u32);
    let position = program.store.checked_expr_types.partition_point(|fact| fact.instruction < instruction as u32);
    program.store.checked_expr_types.insert(position, FullCheckedExprType {
        instruction: instruction as u32, type_id: int,
        purpose: FullCheckedExprPurpose::ProducerContext(ExecutableOwner::Function(IrFunctionId::new(owner).unwrap())),
    });
    let error = FullVerifier::verify(&program).unwrap_err();
    assert!(error.message.contains("ground producer value disagrees with its checked contract") || error.message.contains("compound storage writer has incompatible operands"), "{}", error.message);
}
