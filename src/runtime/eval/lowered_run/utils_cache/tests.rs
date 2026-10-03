use super::*;
use crate::runtime::eval::indexed::full::{FullBuilder, FullProgram};
use crate::sema::check::Checker;

const SOURCE: &str = r#"let base: Str = "original"
pure label(value: Str = base) -> Str { value + base }
proc emit(value: Str = base) [io] -> Str { print $value; value + base }
let first = label
let second = label
let emitter = emit
proc actual_cache(value: Str) [error] -> Any { utils.cache(label, [value]) }
proc unsigned(value: UInt) [io] -> UInt { print $value; value }
let unsigned_callback = unsigned
"#;

fn fixture() -> (Evaluator, Arc<FullProgram>, Span) {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "utils-cache-lifecycle.xsh",
        crate::loader::entry_source_from_text("utils-cache-lifecycle.xsh", SOURCE.to_owned()), Vec::new(),
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = sources.files().first().unwrap().id();
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let solved = Arc::downgrade(&bodies.solved);
    let program = Arc::new(FullBuilder::build_compact(&parsed.arena, &declarations, &bodies,
        SOURCE, Arc::new(sources.clone()), source_id).unwrap());
    drop(parsed); drop(declarations); drop(bodies);
    assert!(solved.upgrade().is_none(), "execution must outlive the source frontend");
    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.indexed_program = Some(program.clone());
    let span = Span::new(source_id, 0, 0);
    for step in 0..program.driver_step_count().unwrap() {
        evaluator.eval_indexed_driver_step(step, span).unwrap().unwrap();
    }
    (evaluator, program, span)
}

fn handle(evaluator: &Evaluator, program: &FullProgram, name: &str) -> RuntimeCallableValue {
    let name = program.symbol_owner().with_current(|| Name::intern(name));
    let Value::Callable(handle) = evaluator.lookup(name).unwrap().value.clone() else {
        panic!("source reference must create an authenticated callable")
    };
    handle
}

fn observed_cache(evaluator: &mut Evaluator, handle: RuntimeCallableValue,
    arguments: Vec<LoweredValue>, span: Span, recursive: bool) -> LoweredValue {
    let program = Arc::clone(handle.program());
    let _symbols = program.symbol_owner().enter();
    let (function, _) = handle.program().function_view_by_id(handle.contract().target).unwrap()
        .execution().unwrap().function_identity().unwrap();
    with_observed_indexed_call_route(function, recursive, ||
        evaluator.eval_prepared_utils_cache(handle, arguments, span)).unwrap()
}

#[test]
fn utils_cache_retains_prepared_callback_captures_defaults_and_actual_workers_after_frontend_drop() {
    std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
        crate::runtime::eval::run_eval(|| {
            for recursive in [false, true] {
                let (mut evaluator, program, span) = fixture();
                let first = handle(&evaluator, &program, "first");
                let second = handle(&evaluator, &program, "second");
                assert_ne!(first, second, "independent source references have independent handles");
                evaluator.define(program.symbol_owner().with_current(|| Name::intern("base")),
                    Binding { value: Value::Str(Arc::from("replacement")), mutable: false });
                assert_eq!(observed_cache(&mut evaluator, first, Vec::new(), span, recursive),
                    LoweredValue::Str(Arc::from("originaloriginal")));
                assert_eq!(evaluator.eval_prepared_utils_cache(second, Vec::new(), span).unwrap(),
                    LoweredValue::Str(Arc::from("originaloriginal")));
                assert_eq!(evaluator.prepared_utils_cache.entries.values().map(Vec::len).sum::<usize>(), 1);
                let emitter = handle(&evaluator, &program, "emitter");
                assert_eq!(observed_cache(&mut evaluator, emitter.clone(), Vec::new(), span, recursive),
                    LoweredValue::Str(Arc::from("originaloriginal")));
                let output = evaluator.stdout.clone();
                assert!(!output.is_empty(), "the cache miss runs the original proc effects");
                evaluator.eval_prepared_utils_cache(emitter, Vec::new(), span).unwrap();
                assert_eq!(evaluator.stdout, output, "the cache hit does not repeat proc effects");
                let label = LoweredFunctionKey::Name(program.symbol_owner().with_current(|| Name::intern("label")));
                let actual_cache = LoweredFunctionKey::Name(program.symbol_owner().with_current(|| Name::intern("actual_cache")));
                let value = with_observed_indexed_call_route(label, recursive, ||
                    evaluator.call_indexed_direct(actual_cache, LoweredFunctionKind::Proc,
                        &[Value::Str(Arc::from("provided"))], span)).unwrap().unwrap();
                assert_eq!(value, Value::Str(Arc::from("providedreplacement")),
                    "the authored host operation invokes its newly created callback environment");
                assert!(evaluator.call_stack.is_empty());
            }
        });
    }).unwrap().join().unwrap();
}

#[test]
fn utils_cache_refuses_missing_foreign_and_stale_callback_receipts_before_cached_results_or_effects() {
    std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
        crate::runtime::eval::run_eval(|| {
            let (mut evaluator, program, span) = fixture();
            let emitter = handle(&evaluator, &program, "emitter");
            observed_cache(&mut evaluator, emitter.clone(), Vec::new(), span, false);
            let output = evaluator.stdout.clone();
            evaluator.indexed_program = None;
            assert_eq!(evaluator.eval_prepared_utils_cache(emitter.clone(), Vec::new(), span).unwrap_err().kind, "indexed-ir");
            let (_, foreign, _) = fixture();
            evaluator.indexed_program = Some(foreign);
            assert_eq!(evaluator.eval_prepared_utils_cache(emitter.clone(), Vec::new(), span).unwrap_err().kind, "indexed-ir");
            let mut stale = (*program).clone();
            stale.test_remove_callable_values();
            let stale = Arc::new(stale);
            let stale_handle = emitter.test_with_program(stale.clone());
            evaluator.indexed_program = Some(stale);
            let key = utils_cache_key("", &[]).unwrap();
            evaluator.prepared_utils_cache.entries.entry(key).or_default().push(PreparedUtilsCacheEntry {
                handle: stale_handle.clone(), value: Value::Str(Arc::from("forged cache hit")),
            });
            assert_eq!(evaluator.eval_prepared_utils_cache(stale_handle, Vec::new(), span).unwrap_err().kind, "indexed-ir");
            assert_eq!(evaluator.stdout, output, "invalid callbacks cannot execute effects");
            assert!(evaluator.call_stack.is_empty());
        });
    }).unwrap().join().unwrap();
}

#[test]
fn utils_cache_validates_original_unsigned_parameters_before_prewarmed_hits_or_proc_effects() {
    std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
        crate::runtime::eval::run_eval(|| {
            for recursive in [false, true] {
                let (mut evaluator, program, span) = fixture();
                let callback = handle(&evaluator, &program, "unsigned_callback");
                assert_eq!(observed_cache(&mut evaluator, callback.clone(), vec![LoweredValue::Int(1)], span, recursive), LoweredValue::Int(1));
                let output = evaluator.stdout.clone();
                let key = utils_cache_key("", &[Value::Int(-1)]).unwrap();
                evaluator.prepared_utils_cache.entries.entry(key).or_default().push(PreparedUtilsCacheEntry {
                    handle: callback.clone(), value: Value::Int(99),
                });
                let error = evaluator.eval_prepared_utils_cache(callback, vec![LoweredValue::Int(-1)], span)
                    .expect_err("cached values cannot bypass the original UInt parameter");
                assert_eq!(error.kind, "type-error");
                assert_eq!(evaluator.stdout, output, "invalid arguments cannot execute proc effects");
                assert!(evaluator.call_stack.is_empty());
            }
        });
    }).unwrap().join().unwrap();
}
