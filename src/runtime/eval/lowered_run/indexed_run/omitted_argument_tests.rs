mod omitted_argument_tests {
    use super::*;
    use crate::runtime::eval::EvalOutput;

    fn execute_after_frontend_drop(source: &str, recursive: bool) -> EvalOutput {
        execute_after_frontend_drop_observed(source, recursive, None)
    }

    fn execute_after_frontend_drop_observed(source: &str, recursive: bool, function: Option<&str>) -> EvalOutput {
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("omitted-call-arguments.xsh", source.to_string());
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let solved = Arc::downgrade(&checked.solved);
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
        let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked)
            .expect("checked omission recipes prepare without evaluating defaults");
        drop(checked);
        drop(parsed);
        assert!(solved.upgrade().is_none(), "a prepared call cannot retain the inference bundle");
        let symbols = evaluator.indexed_program.as_ref().unwrap().symbol_owner().clone();
        let run = || symbols.with_current(|| {
            assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
            let evaluate = || evaluator.try_eval_installed_compact_indexed_only_inner(plan)
                .unwrap_or_else(|_| panic!("the prepared indexed program remains installed"));
            if let Some(function) = function {
                crate::runtime::eval::lowered_run::with_observed_indexed_call_route(
                    LoweredFunctionKey::Name(Name::intern(function)), recursive, evaluate,
                )
            } else { evaluate() }
        });
        if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(run) } else { run() }
    }

    fn assert_interior_expression_default(recursive: bool) {
        crate::runtime::eval::run_eval(|| {
            let source = r#"proc marker(label: Str, value: Int) [io] -> Int { print $label; value }
proc combine(left: Int = marker("default", 4), right: Int = 2) [io] -> Int { left + right }
proc caller() [io] -> Int { combine(right: marker("supplied", 7)) }
print ${caller()}
"#;
            let output = execute_after_frontend_drop(source, recursive);
            assert_eq!(output.status, 0, "{:?}; {:?}", output.diagnostics, output.traceback);
            assert_eq!(output.stdout, b"supplied\ndefault\n11\n", "{:?}; {:?}", output.diagnostics, output.traceback);
            assert!(output.stderr.is_empty(), "{:?}", output.stderr);
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        });
    }

    fn assert_both_routes(source: &str, expected: &[u8]) {
        crate::runtime::eval::run_eval(|| {
            for recursive in [false, true] {
                let output = execute_after_frontend_drop(source, recursive);
                assert!(output.diagnostics.is_empty(), "recursive={recursive}: {:?}; {:?}", output.diagnostics, output.traceback);
                assert_eq!(output.status, 0);
                assert_eq!(output.stdout, expected, "recursive={recursive}");
                assert!(output.stderr.is_empty(), "{:?}", output.stderr);
            }
        });
    }

    #[test]
    fn frame_interior_omission_executes_the_declaration_expression_once_after_supplied_arguments() {
        assert_interior_expression_default(false);
    }

    #[test]
    fn loaded_module_callable_keeps_its_export_contract_after_frontend_disposal() {
        struct ModuleFile(std::path::PathBuf);
        impl Drop for ModuleFile {
            fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
        }
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let file = ModuleFile(std::env::temp_dir().join(format!("xsh-callable-export-{}-{stamp}.xsh", std::process::id())));
        std::fs::write(&file.0, "##! A loaded text renderer.\n## Return the supplied text.\nexport pure render(value: Str) -> Str { value }\n").unwrap();
        let source = format!(r#"type Plugin = module {{ export pure render(value: Str) -> Str }}
proc caller() [fs, error] -> Result[Str] {{
  let plugin = module.load(p"{}")?.require(Plugin)?
  plugin.render("hi")
}}
print ${{caller()?}}
"#, file.0.display());
        crate::runtime::eval::run_eval(|| {
            for recursive in [false, true] {
                let output = execute_after_frontend_drop_observed(&source, recursive, Some("caller"));
                assert!(output.diagnostics.is_empty(), "recursive={recursive}: {:?}; {:?}", output.diagnostics, output.traceback);
                assert_eq!(output.status, 0);
                assert_eq!(output.stdout, b"hi\n", "recursive={recursive}: {:?}; {:?}; {:?}", output.diagnostics, output.traceback, output.stderr);
                assert!(output.stderr.is_empty(), "{:?}", output.stderr);
            }
        });
    }

    #[test]
    fn loaded_module_results_and_proc_exports_keep_local_and_captured_sources_after_frontend_disposal() {
        struct ModuleFiles(std::path::PathBuf);
        impl Drop for ModuleFiles {
            fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
        }
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let files = ModuleFiles(std::env::temp_dir().join(format!("xsh-module-result-{}-{stamp}", std::process::id())));
        std::fs::create_dir_all(&files.0).unwrap();
        let module = files.0.join("renderer.xsh");
        let output_path = files.0.join("rendered.txt");
        std::fs::write(&module, "##! A loaded text renderer.\n## Return the supplied text.\nexport pure render(value: Str) -> Str { value }\n## Write the supplied text.\nexport proc write(destination: Path, value: Str) [fs, error] -> Result[Unit] { fs.write(destination, value)? }\n").unwrap();
        let source = format!(r#"type Plugin = module {{
  export pure render(value: Str) -> Str
  export proc write(destination: Path, value: Str) [fs, error] -> Result[Unit]
}}
let shared = module.load(p"{}")?.require(Plugin)?
proc caller() [fs, error] -> Result[Str] {{
  let local = module.load(p"{}")?.require(Plugin)?
  let first = local.render("retained")
  let second = shared.render(first)
  local.write(p"{}", second)?
  second
}}
print ${{caller()?}}
"#, module.display(), module.display(), output_path.display());
        crate::runtime::eval::run_eval(|| {
            for recursive in [false, true] {
                let output = execute_after_frontend_drop_observed(&source, recursive, Some("caller"));
                assert!(output.diagnostics.is_empty(), "recursive={recursive}: {:?}; {:?}", output.diagnostics, output.traceback);
                assert_eq!(output.status, 0);
                assert_eq!(output.stdout, b"retained\n", "recursive={recursive}: {:?}; {:?}; {:?}", output.diagnostics, output.traceback, output.stderr);
                assert!(output.stderr.is_empty(), "{:?}", output.stderr);
                assert_eq!(std::fs::read(&output_path).unwrap(), b"retained");
                std::fs::remove_file(&output_path).unwrap();
            }
        });
    }

    #[test]
    fn live_mutable_capture_keeps_nested_defer_writes_after_frontend_disposal() {
        assert_both_routes(r#"var observed = 0
proc record() [] -> Unit { observed = 1 }
proc completed() [error] -> Result[Int] { defer record(); 7 }
print ${completed()?}
print $observed
"#, b"7\n1\n");
    }

    #[test]
    fn live_mutable_capture_reads_nested_writes_and_preserves_original_binding_under_shadowing() {
        assert_both_routes(r#"var observed = 0
proc record() [] -> Unit { observed += 1 }
proc completed() [] -> Int { record(); observed }
proc shadowed() [] -> Int { let observed = 100; completed() }
print ${shadowed()}
print ${completed()}
print $observed
"#, b"1\n2\n2\n");
    }

    #[test]
    fn live_mutable_capture_default_reads_the_cell_after_supplied_argument_mutation() {
        assert_both_routes(r#"var observed = 0
proc mutate() [] -> Int { observed = 4; 7 }
proc combine(left: Int = observed, right: Int = 0) [] -> Int { left + right }
proc caller() [] -> Int { combine(right: mutate()) }
print ${caller()}
print $observed
"#, b"11\n4\n");
    }

    #[test]
    fn live_mutable_capture_scalar_reads_observe_nested_writes_within_one_expression() {
        assert_both_routes(r#"var observed = 0
var enabled = false
proc number() [] -> Int { observed = 4; 7 }
proc flag() [] -> Bool { enabled = true; true }
proc completed() [] -> Int { number() + observed }
proc ready() [] -> Bool { flag() and enabled }
print ${completed()}
print ${ready()}
"#, b"11\ntrue\n");
    }

    #[test]
    fn live_mutable_capture_stored_callable_reads_and_writes_its_original_cell() {
        assert_both_routes(r#"var observed = 0
proc update() [] -> Int { observed += 1; observed }
let alias = update
observed = 4
proc caller() [] -> Int { let observed = 100; alias.call() }
print ${caller()}
print $observed
"#, b"5\n5\n");
    }

    #[test]
    fn live_mutable_capture_stored_default_observes_supplied_mutation_in_original_environment() {
        assert_both_routes(r#"var observed = 0
proc mutate() [] -> Int { observed = 4; 7 }
proc combine(left: Int = observed, right: Int = 0) [] -> Int { left + right }
let alias = combine
proc caller() [] -> Int { let observed = 100; alias.call(right: mutate()) }
print ${caller()}
print $observed
"#, b"11\n4\n");
    }

    #[test]
    fn live_mutable_producer_capture_preserves_shared_cell_across_pulls_after_frontend_disposal() {
        assert_both_routes(r#"var observed = 0
stream rows() [] -> Stream[Int] {
    yield observed
    observed = observed + 1
    yield observed
}
let pending = rows()
observed = 4
for item in pending {
    print $item
    observed = observed + 10
}
print $observed
"#, b"4\n15\n25\n");
    }

    #[test]
    fn live_mutable_producer_capture_cancellation_keeps_deferred_writes_after_frontend_disposal() {
        assert_both_routes(r#"var observed = 0
proc record() [] -> Unit { observed = observed + 1 }
stream rows() [error] -> Stream[Int] {
    defer record()
    yield observed
    observed = 100
    yield observed
}
proc caller() [io, error] -> Unit {
    for item in rows() {
        print $item
        observed = 4
        break
    }
}
caller()
print $observed
"#, b"0\n5\n");
    }

    #[test]
    fn recursive_interior_omission_executes_the_declaration_expression_once_after_supplied_arguments() {
        assert_interior_expression_default(true);
    }

    #[test]
    fn omitted_defaults_keep_outer_dependencies_and_distinguish_supplied_null() {
        let source = r#"pure initial() -> Int { 4 }
let lexical = initial()
pure dependency() -> Int { lexical + 1 }
pure combine(left: Int = dependency(), right: Int = 2) -> Int { left + right }
proc fallback() [io] -> Str? { print "label-default"; "default" }
proc label(value: Str? = fallback(), other: Int = 0) [io] -> Str { let _ = other; value ?? "supplied-null" }
proc caller() [io] -> Unit {
    let lexical = 100
    let _ = lexical
    print ${combine(right: 7)} ${label(other: 9)} ${label(value: null, other: 9)}
}
caller()
"#;
        assert_both_routes(source, b"label-default\n12 default supplied-null\n");
    }

    #[test]
    fn callable_alias_method_omissions_keep_prepared_defaults_after_frontend_drop() {
        let source = r#"const lexical = 4
pure dependency() -> Int { lexical + 1 }
pure combine(left: Int = dependency(), right: Int = 2) -> Int { left + right }
proc caller() [io] -> Unit {
    let alias = combine
    print ${alias.call(right: 7)}
}
caller()
"#;
        assert_both_routes(source, b"12\n");
    }

    #[test]
    fn saved_callable_receiver_keeps_captured_defaults_after_supplied_argument_effects() {
        let source = r#"pure initial() -> Int { 4 }
let lexical = initial()
proc marker(label: Str, value: Int) [io] -> Int { print $label; value }
proc combine(left: Int = marker("default", lexical), right: Int = 2) [io] -> Int { left + right }
proc caller() [io] -> Unit {
    let alias = combine
    let lexical = 100
    let _ = lexical
    print ${alias.call(right: marker("supplied", 7))}
}
caller()
"#;
        assert_both_routes(source, b"supplied\ndefault\n11\n");
    }

    #[test]
    fn saved_conditional_callable_receiver_keeps_original_captured_default_authority() {
        let source = r#"pure initial() -> Int { 4 }
let lexical = initial()
proc marker(label: Str, value: Int) [io] -> Int { print $label; value }
proc combine(left: Int = marker("default", lexical), right: Int = 2) [io] -> Int { left + right }
proc caller(choice: Bool, nested: Bool) [io] -> Unit {
    let alias = if choice { (if nested { (combine) } else { (combine) }) } else { (combine) }
    let lexical = 100
    let _ = lexical
    print ${alias.call(right: marker("supplied", 7))}
}
caller(true, true)
caller(true, false)
caller(false, true)
"#;
        assert_both_routes(source, b"supplied\ndefault\n11\nsupplied\ndefault\n11\nsupplied\ndefault\n11\n");
    }

    #[test]
    fn saved_conditional_captured_callable_branches_keep_original_defaults_and_supplied_order() {
        let source = r#"pure initial() -> Int { 4 }
let lexical = initial()
proc marker(label: Str, value: Int) [io] -> Int { print $label; value }
proc combine(left: Int = marker("default", lexical), right: Int = 2) [io] -> Int { left + right }
let first = combine
let second = combine
proc caller(choice: Bool, nested: Bool) [io] -> Unit {
    let alias = if choice { (if nested { (first) } else { (second) }) } else { (second) }
    let first = 9
    let lexical = 100
    let _ = first
    let _ = lexical
    print ${alias.call(right: marker("supplied", 7))}
}
caller(true, true)
caller(true, false)
caller(false, true)
"#;
        crate::runtime::eval::run_eval(|| {
            for recursive in [false, true] {
                let output = execute_after_frontend_drop_observed(source, recursive, Some("caller"));
                assert!(output.diagnostics.is_empty(), "recursive={recursive}: {:?}", output.diagnostics);
                assert_eq!(output.status, 0);
                assert_eq!(output.stdout, b"supplied\ndefault\n11\nsupplied\ndefault\n11\nsupplied\ndefault\n11\n");
                assert!(output.stderr.is_empty());
            }
        });
    }

    #[test]
    fn saved_conditional_captured_pure_callable_branches_keep_original_immutable_allocations() {
        let source = r#"pure initial() -> Int { 4 }
let lexical = initial()
pure combine(left: Int = lexical, right: Int = 2) -> Int { left + right }
let first = combine
let second = combine
pure caller(choice: Bool) -> Int {
    let alias = if choice { (first) } else { (second) }
    let lexical = 100
    let _ = lexical
    alias.call(right: 7)
}
print ${caller(true)} ${caller(false)}
"#;
        crate::runtime::eval::run_eval(|| {
            for recursive in [false, true] {
                let output = execute_after_frontend_drop_observed(source, recursive, Some("caller"));
                assert!(output.diagnostics.is_empty(), "recursive={recursive}: {:?}", output.diagnostics);
                assert_eq!(output.status, 0);
                assert_eq!(output.stdout, b"11 11\n");
                assert!(output.stderr.is_empty());
            }
        });
    }

    #[test]
    fn omitted_stream_defaults_wait_for_first_pull_and_supplied_arguments_remain_eager() {
        let source = r#"proc marker(label: Str, value: Int) [io] -> Int { print $label; value }
stream rows(left: Int = marker("default", 4), right: Int = 2) [io] -> Stream[Int] {
    print body
    yield left + right
}
proc caller() [io] -> Unit {
    let _ = rows(right: 1)
    let omitted = rows(right: marker("supplied", 7))
    let supplied = rows(left: marker("provided", 8), right: 3)
    print before
    for item in omitted { print $item }
    for item in supplied { print $item }
}
caller()
"#;
        assert_both_routes(source, b"supplied\nprovided\nbefore\ndefault\nbody\n11\nbody\n11\n");
    }

    #[test]
    fn omitted_expression_default_propagation_runs_cleanup_and_skips_later_defaults() {
        let source = r#"error DefaultError = invalid(message: Str)
proc failing() [io, error] -> Result[Int] {
    defer { print cleanup }
    Err(DefaultError.invalid(message: "default failed"))
}
proc later() [io] -> Int { print later; 2 }
proc choose(first: Int = failing()?, other: Int = later(), supplied: Int = 0) [io, error] -> Result[Int] {
    Ok(first + other + supplied)
}
proc caller() [io, error] -> Unit {
    let captured = try { choose(supplied: 9)? }
    print ${match captured { Ok(_) => false, Err(_) => true }}
    print ${choose(8, 9, 0)?}
}
caller()
"#;
        assert_both_routes(source, b"cleanup\ntrue\n17\n");
    }
}
