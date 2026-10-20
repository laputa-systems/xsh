use super::super::*;
use crate::runtime::eval::Evaluator;
use crate::sema::check::Checker;

fn fixture(source: &str) -> FullProgram {
    let name = "native-internal-effects.xsh";
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        name, crate::loader::entry_source_from_text(name, source.to_string()), Vec::new(),
    );
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena_with_sources_and_options(&parsed.arena, &sources, source_id,
        crate::sema::check::CheckOptions { reveal_types: true, embedded_bodies: true, ..Default::default() });
    let declarations = Checker::compact_declarations(&parsed.arena, checked);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    FullBuilder::build_compact(&parsed.arena, &declarations, source, Arc::new(sources), source_id).unwrap()
}

fn on_large_stack(test: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(test).unwrap().join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
}

fn execute_both(program: &FullProgram) {
    let program = Arc::new(program.clone());
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
        evaluator.indexed_program = Some(Arc::clone(&program));
        evaluator.force_frames = force_frames;
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::new(program.store.source_id, 0, 0)).unwrap().unwrap();
        }
    }
}

fn function(program: &FullProgram, name: &str) -> usize {
    program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == name).unwrap()
}

fn without_effect(program: &FullProgram, owner: usize, effect: Effect) -> FullProgram {
    let FullCallableContract::Ground(signature) = program.store.functions[owner].contract else { panic!("ground function") };
    let mut signature = program.store.semantic.to_signature_with_budget(signature, 0,
        &mut crate::runtime::eval::indexed::semantic::TypeMaterializationBudget::default()).unwrap();
    signature.effects.as_mut().unwrap().retain(|candidate| *candidate != effect);
    let mut forged = program.clone();
    forged.store.functions[owner].contract = FullCallableContract::Ground(
        SemanticPoolBuilder::default().intern_signature(&mut forged.store.semantic, &signature).unwrap());
    forged
}

#[test]
fn physical_process_spawn_and_wait_contracts_preserve_results() {
    on_large_stack(|| {
        fixture(r#"
proc wrapped_launch() [process] -> Result[Result[ProcessHandle, ProcessError]] {
    let started = spawn run true
    Ok(started)
}
"#);
        let source = r#"
proc launch() [process] -> Result[ProcessHandle, ProcessError] {
    spawn run true
}
proc launch_plan(plan: Command) [process] -> Result[ProcessHandle, ProcessError] {
    spawn plan
}
pure build_argv(target: Str, words: List[Str]) -> Command {
    process.command_argv(target, words)
}
proc launch_builder() [process, error] -> Result[ProcessHandle, ProcessError] {
    spawn process.command { timeout = 2s; run true }
}
let child = launch()?
let status = wait child?
assert status.ok
let plan = build_argv("sh", ["sh", "-c", "exit 1"])
let planned = launch_plan(plan)?
let built = launch_builder()?
let statuses = wait [planned, built]?
assert !statuses[0].ok
assert statuses[1].ok
let empty = wait []?
assert empty.len() == 0
"#;
        let mut program = fixture(source);
        FullVerifier::verify(&program).unwrap();
        let owner = function(&program, "build_argv");
        let function = program.store.functions[owner];
        for parameter in 0..function.params.len as usize {
            program.store.slot_type_facts[function.slot_type_facts.start as usize + parameter] = IR_NONE;
        }
        let range = program.store.function_instruction_range(owner).unwrap();
        program.store.checked_expr_types.retain(|fact| fact.is_contract() || !range.contains(&(fact.instruction as usize)));
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
    });
}

fn process_parameter_mutation(program: &FullProgram, owner: &str, from: u32, to: u32) -> FullProgram {
    let owner = function(program, owner);
    let decoder = function_view_decoder(&program.store, owner);
    let instruction = program.store.function_instruction_range(owner).unwrap().find(|&instruction| {
        matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(slot)) if slot == from as usize)
    }).unwrap();
    let mut forged = program.clone();
    forged.store.extra[program.store.data[instruction].range().start as usize] = to;
    forged
}

#[test]
fn physical_process_producers_reject_wrong_handles_commands_and_run_operands() {
    on_large_stack(|| {
        let source = r#"
proc launch_words(target: Str, words: List[Str], input: Bytes, limit: Duration, cpus: Int, accepted: List[Int], paths: List[Path], wrong: List[Bytes], erased: Any) [process, error] -> Result[ProcessHandle, ProcessError] {
    spawn run --timeout=limit --cpumax=cpus --accept=accepted XSH_PROCESS_PATH=(paths) $target @words < (input)
}
proc launch_command(plan: Command, wrong: Str, erased: Any) [process] -> Result[ProcessHandle, ProcessError] {
    spawn plan
}
proc await_one(handle: ProcessHandle, wrong: Str, erased: Any) [process] -> Result[Status, ProcessError] {
    wait handle
}
proc await_many(handles: List[ProcessHandle], wrong: List[Int], erased: Any) [process] -> Result[List[Status], ProcessError] {
    wait handles
}
proc build_plan(limit: Duration, wrong: Bool) [process, error] -> Result[ProcessHandle, ProcessError] {
    spawn process.command { timeout = limit; run true }
}
"#;
        let mut program = fixture(source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        for (name, from, to, message) in [
            ("launch_words", 0, 8, "generic call substitution contains dynamic type"),
            ("launch_words", 1, 7, "process word disagrees with its physical conversion role"),
            ("launch_words", 2, 7, "process word disagrees with its physical conversion role"),
            ("launch_words", 3, 4, "ground producer value disagrees with its checked contract"),
            ("launch_words", 4, 3, "ground producer value disagrees with its checked contract"),
            ("launch_words", 5, 7, "ground producer value disagrees with its checked contract"),
            ("launch_words", 6, 7, "process word disagrees with its physical conversion role"),
            ("launch_command", 0, 1, "ground producer value disagrees with its checked contract"),
            ("launch_command", 0, 2, "ground producer value disagrees with its checked contract"),
            ("await_one", 0, 1, "wait target is neither ProcessHandle nor List[ProcessHandle]"),
            ("await_one", 0, 2, "generic call substitution contains dynamic type"),
            ("await_many", 0, 1, "wait target is neither ProcessHandle nor List[ProcessHandle]"),
            ("await_many", 0, 2, "generic call substitution contains dynamic type"),
            ("build_plan", 0, 1, "process option disagrees with its canonical contract"),
        ] {
            let error = FullVerifier::verify(&process_parameter_mutation(&program, name, from, to)).unwrap_err();
            assert!(error.message.contains(message), "{name} parameter {from}->{to}: {error:?}");
        }
        for name in ["launch_words", "launch_command", "await_one", "await_many", "build_plan"] {
            let error = FullVerifier::verify(&without_effect(&program, function(&program, name), Effect::Process)).unwrap_err();
            assert!(error.message.contains("requires effect process"), "{name}: {error:?}");
        }
        let error = FullVerifier::verify(&without_effect(&program, function(&program, "launch_words"), Effect::Error)).unwrap_err();
        assert!(error.message.contains("requires effect error"), "{error:?}");
    });
}

#[test]
fn physical_process_results_match_uncalled_owner_contracts() {
    on_large_stack(|| {
        let source = r#"
proc await_one(handle: ProcessHandle) [process] -> Result[Status, ProcessError] { wait handle }
proc await_many(handles: List[ProcessHandle]) [process] -> Result[List[Status], ProcessError] { wait handles }
proc launch_command(plan: Command) [process] -> Result[ProcessHandle, ProcessError] { spawn plan }
"#;
        let mut program = fixture(source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        for name in ["await_one", "await_many", "launch_command"] {
            let owner = function(&program, name);
            let FullCallableContract::Ground(id) = program.store.functions[owner].contract else { panic!("ground process function") };
            let mut signature = program.store.semantic.to_signature_with_budget(id, 0,
                &mut crate::runtime::eval::indexed::semantic::TypeMaterializationBudget::default()).unwrap();
            signature.return_ty = Box::new(Type::Result(Box::new(Type::Bool), Box::new(Type::ProcessError)));
            let mut forged = program.clone();
            let signature = SemanticPoolBuilder::default().intern_signature(&mut forged.store.semantic, &signature).unwrap();
            forged.store.functions[owner].contract = FullCallableContract::Ground(signature);
            forged.store.functions[owner].checked_return_type = forged.store.semantic.signature_return_type(signature).unwrap().raw();
            let error = FullVerifier::verify(&forged).unwrap_err();
            assert_eq!(error.message, "ground producer value disagrees with its checked contract", "{name}: {error:?}");
        }
    });
}

#[test]
fn physical_process_builder_preserves_the_canonical_record_environment_boundary() {
    on_large_stack(|| {
        let source = r#"
proc launch_with_env(environment: Record, erased: Any, wrong: Bool) [process, error] -> Result[ProcessHandle, ProcessError] {
    spawn process.command {
        env = environment
        run sh -c r"""test "$XSH_RECORD_ENV" = preserved"""
    }
}
let child = launch_with_env({XSH_RECORD_ENV: "preserved"}, {}, false)?
let status = wait child?
assert status.ok
"#;
        let program = fixture(source);
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        for slot in [1, 2] {
            let error = FullVerifier::verify(&process_parameter_mutation(&program, "launch_with_env", 0, slot)).unwrap_err();
            assert_eq!(error.message, "process option disagrees with its canonical contract", "{error:?}");
        }
    });
}

#[test]
fn environment_assignment_proves_native_effects_and_closed_value_domain() {
    on_large_stack(|| {
        let source = r#"
proc configure(text: Str, paths: List[Path], wrong: List[Int], erased: Any) [env, error] -> Result[Str] {
    let _ = wrong
    let _ = erased
    e"XSH_NATIVE_TEXT" = text
    e"XSH_NATIVE_NUMBER" = 8080
    e"XSH_NATIVE_PATHS" = paths
    Ok(e"XSH_NATIVE_TEXT"?)
}
assert configure("kept", [p"/one", p"/two"], [1], {})? == "kept"
"#;
        let mut program = fixture(source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "configure");
        let decoder = function_view_decoder(&program.store, owner);
        let value = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
            let schema::ExprView::ModuleCall { op: RuntimeOp::EnvSet, mut args, .. } = decoder.expression_view(instruction).ok()? else { return None };
            args.next().unwrap().unwrap().unwrap();
            Some(args.next().unwrap().unwrap().unwrap().raw() as usize)
        }).unwrap();
        assert!(matches!(decoder.expression_view(value).unwrap(), schema::ExprView::Param(0)));
        for slot in [2, 3] {
            let mut forged = program.clone();
            forged.store.extra[program.store.data[value].range().start as usize] = slot;
            let error = FullVerifier::verify(&forged).unwrap_err();
            assert!(error.message.contains("environment assignment value has no closed conversion domain"), "{error:?}");
        }
        let error = FullVerifier::verify(&without_effect(&program, owner, Effect::Env)).unwrap_err();
        assert!(error.message.contains("requires effect env"), "{error:?}");
    });
}

#[test]
fn managed_release_proves_canonical_resource_and_native_effects() {
    on_large_stack(|| {
        let source = r#"
proc publish(site: Path) [fs, error] -> Result[Int] {
    with root = fs.open_root(site)?, held = fs.lock(fp"{site}/.lock")? {
        root.write(p"value", "kept")
    }
    let text = with root = fs.open_root(site)? { assert root.read_text(p"value")? == "kept"; "kept" }?
    Ok(text.byte_len())
}
tempdir site { assert publish(site)? == 4 }
"#;
        let mut program = fixture(source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "publish");
        let decoder = function_view_decoder(&program.store, owner);
        let payload = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
            let schema::ExprView::Ok(value) = decoder.expression_view(instruction).ok()? else { return None };
            (program.store.tags[value.raw() as usize] == FullTag::ExprStr).then_some(value.raw() as usize)
        }).unwrap();
        let integer = program.store.tags.iter().position(|tag| *tag == FullTag::ExprInt).unwrap();
        let mut wrong_payload = program.clone();
        wrong_payload.store.tags[payload] = FullTag::ExprInt;
        wrong_payload.store.data[payload] = program.store.data[integer];
        let error = FullVerifier::verify(&wrong_payload).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees") || error.message.contains("storage contract"), "{error:?}");
        let handover = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
            let schema::StmtView::Assign { value, .. } = decoder.statement_view(instruction).ok()? else { return None };
            (program.store.tags[value.raw() as usize] == FullTag::ExprBool).then_some(value.raw() as usize)
        }).unwrap();
        let mut wrong_handover = program.clone();
        wrong_handover.store.tags[handover] = FullTag::ExprInt;
        wrong_handover.store.data[handover] = program.store.data[integer];
        let error = FullVerifier::verify(&wrong_handover).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees") || error.message.contains("storage contract"), "{error:?}");
        for (from, to) in [(RuntimeOp::FsCloseRootIfOpen, RuntimeOp::FsUnlockIfHeld), (RuntimeOp::FsUnlockIfHeld, RuntimeOp::FsCloseRootIfOpen)] {
            let instruction = program.store.function_instruction_range(owner).unwrap().find(|&instruction| {
                matches!(decoder.expression_view(instruction), Ok(schema::ExprView::ModuleCall { op, .. }) if op == from)
            }).unwrap();
            let mut forged = program.clone();
            let pool = program.store.extra[program.store.data[instruction].range().start as usize] as usize;
            forged.store.runtime_ops[pool] = to;
            let error = FullVerifier::verify(&forged).unwrap_err();
            assert!(error.message.contains("native operation arguments have no matching contract"), "{error:?}");
        }
        let error = FullVerifier::verify(&without_effect(&program, owner, Effect::Fs)).unwrap_err();
        assert!(error.message.contains("requires effect fs"), "{error:?}");
    });
}

#[test]
fn typed_module_dynamic_calls_preserve_export_contracts() {
    on_large_stack(|| {
        let work = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        std::fs::create_dir_all(&work).unwrap();
        let directory = tempfile::tempdir_in(work).unwrap();
        let module = directory.path().join("calculator.xsh");
        std::fs::write(&module, r#"##! Checked dynamic module calls.
## Adds a defaulted amount.
export pure add(value: Int = 4, by: Int = 2) -> Int { value + by }
## Reads an environment value before observing the input.
export proc observe(value: Int) [env] -> Int { let _ = e"PATH"; value + 1 }
"#).unwrap();
        let source = format!(r#"
type Calculator = module {{
    export pure add(value: Int = 4, by: Int = 2) -> Int
    export proc observe(value: Int) [env] -> Int
}}
proc apply(plugin: Calculator, value: Int, wrong: Str) [env] -> Int {{
    let _ = wrong
    let added = plugin.add(by: value)
    plugin.observe(added)
}}
pure apply_pure(plugin: Calculator) -> Int {{ plugin.add(4, by: 3) }}
pure apply_trailing(plugin: Calculator) -> Int {{ plugin.add(5) }}
let plugin = module.load(p{module:?})?.require(Calculator)?
assert apply(plugin, 4, "wrong") == 9
assert apply_pure(plugin) == 7
assert apply_trailing(plugin) == 7
"#);
        let program = fixture(&source);
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "apply");
        let decoder = function_view_decoder(&program.store, owner);
        let default_args = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
            let schema::ExprView::DynamicCall { args, .. } = decoder.expression_view(instruction).ok()? else { return None };
            let block = args.block_identity_for_test();
            args.into_iter().any(|argument| matches!(argument, Ok(super::super::decode::CallArgView::Default(0)))).then_some(block)
        }).unwrap();
        let words = program.store.blocks[default_args.index()].instructions.bounds(program.store.extra.len()).unwrap();
        assert_eq!(program.store.extra[words.start + 1..words.start + 3], [2, 0]);
        let mut wrong_default = program.clone();
        wrong_default.store.extra[words.start + 2] = 1;
        let error = FullVerifier::verify(&wrong_default).unwrap_err();
        assert!(error.message.contains("default argument has no parameter default"), "{error:?}");
        let error = FullVerifier::verify(&without_effect(&program, owner, Effect::Env)).unwrap_err();
        assert!(error.message.contains("requires effect env"), "{error:?}");
        let observe = program.store.function_instruction_range(owner).unwrap().find(|&instruction| {
            matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Field { name: "observe", .. }))
        }).unwrap();
        let pure_owner = function(&program, "apply_pure");
        let pure_decoder = function_view_decoder(&program.store, pure_owner);
        let pure_field = program.store.function_instruction_range(pure_owner).unwrap().find_map(|instruction| {
            let schema::ExprView::DynamicCall { callee, .. } = pure_decoder.expression_view(instruction).ok()? else { return None };
            Some(callee.raw() as usize)
        }).unwrap();
        let mut impure = program.clone();
        impure.store.extra[program.store.data[pure_field].range().start as usize + 1] =
            program.store.extra[program.store.data[observe].range().start as usize + 1];
        let error = FullVerifier::verify(&impure).unwrap_err();
        assert!(error.message.contains("pure executable body contains an impure operation"), "{error:?}");
    });
}

#[test]
fn typed_module_dynamic_calls_prove_arguments_without_result_demand() {
    on_large_stack(|| {
        let mut program = fixture(r#"
type Calculator = module { export pure add(value: Int) -> Int; export pure wrong(value: Int) -> Str }
proc apply(plugin: Calculator, value: Int, wrong: Str) [] -> Int { let _ = wrong; let _ = "wrong"; plugin.add(value) }
"#);
        assert!(program.store.registered_calls.is_empty());
        assert!(!program.store.tags.iter().any(|tag| matches!(tag, FullTag::ExprOk | FullTag::ExprErr | FullTag::ExprTypedCall)));
        program.store.checked_expr_types.clear();
        FullVerifier::verify(&program).unwrap();
        let owner = function(&program, "apply");
        let decoder = function_view_decoder(&program.store, owner);
        let (arguments, callee, argument) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
            let schema::ExprView::DynamicCall { callee, mut args, .. } = decoder.expression_view(instruction).ok()? else { return None };
            let block = args.block_identity_for_test();
            let super::super::decode::CallArgView::Single(value) = args.next().unwrap().unwrap() else { panic!("single argument") };
            Some((block, callee.raw() as usize, value.raw() as usize))
        }).unwrap();
        assert!(matches!(decoder.expression_view(argument).unwrap(), schema::ExprView::Param(1)));
        let mut wrong = program.clone();
        wrong.store.extra[program.store.data[argument].range().start as usize] = 2;
        let error = FullVerifier::verify(&wrong).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees"), "{error:?}");
        let words = program.store.blocks[arguments.index()].instructions.bounds(program.store.extra.len()).unwrap();
        assert_eq!(program.store.extra[words.start..words.start + 2], [1, 0]);
        let mut wrong_splice = program.clone();
        wrong_splice.store.extra[words.start + 1] = 1;
        let error = FullVerifier::verify(&wrong_splice).unwrap_err();
        assert!(error.message.contains("splice has no rest parameter"), "{error:?}");
        let schema::ExprView::Field { base, .. } = decoder.expression_view(callee).unwrap() else { panic!("module field") };
        let mut nonmodule = program.clone();
        nonmodule.store.extra[program.store.data[base.raw() as usize].range().start as usize] = 2;
        let error = FullVerifier::verify(&nonmodule).unwrap_err();
        assert!(error.message.contains("unknown effect contract"), "{error:?}");
        let words = program.store.data[callee].range().bounds(program.store.extra.len()).unwrap();
        let name = program.store.strings.iter().position(|range| program.store.string_bytes[range.bounds(program.store.string_bytes.len()).unwrap()] == *b"wrong").unwrap();
        let mut wrong_result = program.clone();
        wrong_result.store.extra[words.start + 1] = IrStringId::new(name).unwrap().raw();
        let error = FullVerifier::verify(&wrong_result).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees"), "{error:?}");
    });
}

#[test]
fn unknown_dynamic_calls_keep_unknown_effect_contracts() {
    on_large_stack(|| {
        let mut program = fixture("proc erased(action: Proc) -> Result[Any] { action.call() }\n");
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        let owner = function(&program, "erased");
        let FullCallableContract::Ground(signature) = program.store.functions[owner].contract else { panic!("ground function") };
        let mut signature = program.store.semantic.to_signature_with_budget(signature, 0,
            &mut crate::runtime::eval::indexed::semantic::TypeMaterializationBudget::default()).unwrap();
        signature.effects = Some(Vec::new());
        program.store.functions[owner].contract = FullCallableContract::Ground(
            SemanticPoolBuilder::default().intern_signature(&mut program.store.semantic, &signature).unwrap());
        let error = FullVerifier::verify(&program).unwrap_err();
        assert!(error.message.contains("unknown effect contract"), "{error:?}");
    });
}
