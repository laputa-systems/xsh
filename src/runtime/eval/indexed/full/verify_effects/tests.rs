use crate::runtime::eval::indexed::full::decode::ValueCheckView;
use super::super::*;
use crate::runtime::eval::Evaluator;
use crate::sema::check::{CheckOptions, Checker};
use crate::syntax::parser::Parser;

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
            let flow = evaluator.eval_indexed_driver_step(step, Span::new(program.store.source_id, 0, 0)).unwrap().unwrap();
            assert!(matches!(&flow, None | Some(crate::runtime::eval::Flow::Continue(_))), "frames={force_frames} step={step}: {flow:?}");
        }
    }
}

#[test]
fn driver_execution_oracle_rejects_uncaught_propagation() {
    on_large_stack(|| {
        let program = fixture("error Failure { Bad }\npure failed() -> Result[Unit] { Err(Failure.Bad(\"bad\")) }\nfailed()?\n");
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| execute_both(&program)));
        assert!(outcome.is_err(), "an escaping script error cannot qualify successful execution");
    });
}

#[test]
fn generic_empty_factories_preserve_material_domains_and_unit_completion() {
    on_large_stack(|| {
        let source = r#"
pure map_sample(value) -> Unit {
    var entries = map.empty()
    entries["item"] = value
    let _ = entries
}
pure set_sample(value) -> Unit {
    var seen = set.empty()
    seen = seen.add(value)
    let _ = seen
}
pure map_value(value) {
    var entries = map.empty()
    entries["item"] = value
    entries
}
pure set_value(value) {
    var seen = set.empty()
    seen = seen.add(value)
    seen
}
let number: Unit = map_sample(7)
let text: Unit = map_sample("kept")
let set_number: Unit = set_sample(7)
let set_text: Unit = set_sample("kept")
assert number == text
assert set_number == set_text
assert map_value(7).values() == [7]
assert map_value("kept").values() == ["kept"]
assert map_value({field: 7}).values() == [{field: 7}]
assert set_value(7).to_list() == [7]
assert set_value("kept").to_list() == ["kept"]
var order = 0
proc step(value) [] -> Unit {
    order = order * 10 + 1
    let _ = value
}
step(7)
step("kept")
assert order == 11
"#;
        let program = fixture(source);
        execute_both(&program);
        let constrained = program.store.scheme_type_parameters.iter().enumerate()
            .filter_map(|(index, parameter)| parameter.map_key.then_some(index)).collect::<Vec<_>>();
        assert!(!constrained.is_empty());
        let mut missing = program.clone();
        for &index in &constrained { missing.store.scheme_type_parameters[index].map_key = false; }
        assert!(FullVerifier::verify(&missing).is_err());
        let mut malformed = program.clone();
        malformed.store.scheme_type_parameters[constrained[0]].iteration_policy = super::super::schemes::FullIterationPolicy::PipelineTransport as u8;
        assert!(FullVerifier::verify(&malformed).is_err());
        for source in [
            "pure sample(value) -> Bool { var entries = map.empty(); entries[\"item\"] = value; let _ = entries }\n",
            "pure sample(value) -> Result[Int] { var seen = set.empty(); seen = seen.add(value); let _ = seen }\n",
            "pure sample(value) -> Unit { var entries = map.empty(); entries[\"item\"] = value; entries[\"item\"] = true; let _ = entries }\nsample(\"wrong\")\n",
            "pure sample(value) -> Unit { var seen = set.empty(); seen = seen.add(value); let _ = seen }\nsample({field: 7})\n",
            "type Row = {field: Int}\npure sample(value: Row) -> Unit { var seen = set.empty(); seen = seen.add(value); let _ = seen }\nsample({field: 7})\n",
        ] {
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{source}: {:?}", checked.diagnostics);
        }
    });
}

#[test]
fn result_context_argument_forms_preserve_err_and_ok_values() {
    let source = |arguments: &str| format!(r#"
error TestBaseError = Base
let original: Result[Int, TestBaseError] = Err(TestBaseError.Base("base message"))
let decorated = original.context({arguments})
let same_family = match decorated {{
  Err(TestBaseError.Base) => true
  _ => false
}}
assert same_family
if let Err(problem) = decorated {{
  assert "base message" in problem.message
  assert "extra" in problem.message
}} else {{ assert false }}
if let Err(problem) = original {{
  assert problem.message == "base message"
}} else {{ assert false }}
let success: Result[Int, TestBaseError] = Ok(17)
match success.context({arguments}) {{
  Ok(value) => assert value == 17
  _ => assert false
}}
"#);
    let two_arguments = fixture(&source("\"wrapped\", \"extra\""));
    execute_both(&two_arguments);
    let rejected = source("\"wrapped\", \"extra\", 7");
    let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &rejected);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena_with_options(&parsed.arena, &rejected, crate::sema::check::CheckOptions::for_execution());
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckArity)), "{:?}", checked.diagnostics);
}

fn function(program: &FullProgram, name: &str) -> usize {
    program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == name).unwrap()
}

fn without_effect(program: &FullProgram, owner: usize, effect: Effect) -> FullProgram {
    let FullCallableContract::Ground(signature) = program.store.functions[owner].contract else { panic!("ground function") };
    let mut signature = program.store.semantic.to_signature_with_budget(signature, 0,
        &mut crate::runtime::eval::indexed::semantic::TypeMaterializationBudget::default()).unwrap();
    let mut effects = signature.effects.concrete().unwrap().unwrap();
    effects.retain(|candidate| *candidate != effect);
    signature.effects = crate::sema::types::CallableEffects::finite(effects);
    let mut forged = program.clone();
    forged.store.functions[owner].contract = FullCallableContract::Ground(
        SemanticPoolBuilder::default().intern_signature(&mut forged.store.semantic, &signature).unwrap());
    forged
}

#[test]
fn result_unit_tail_forwarding_retains_interior_error_effects() {
    let program = fixture(r#"
pure completed() -> Result[Unit] { Ok() }
error TailFailure = Failed(message: Str)
pure parameter(value: Result[Unit, TailFailure], other: Result[Unit]) -> Result[Unit, TailFailure] { value }
proc choose_failure() [] -> Bool { true }
let saved: Result[Unit, TailFailure] = if choose_failure() { Err(TailFailure.Failed("saved")) } else { Ok() }
pure captured() -> Result[Unit, TailFailure] { saved }
assert parameter(Ok(), saved) is Ok(_)
assert parameter(saved, Ok()) is Err(_)
assert captured() is Err(_)
var order = 0
proc fail_tail() [] -> Result[Unit] {
    order = order * 10 + 1
    Err(TailFailure.Failed("stopped"))
}
proc forward_failure() [] {
    defer { order = order * 10 + 2 }
    errdefer { order = order * 10 + 3 }
    fail_tail()
}
proc forward_success() [] {
    errdefer { order = 99 }
    completed()
}
proc inferred() [] { completed() }
proc explicit() [] -> Result[Unit] { completed() }
proc interior() [error] -> Result[Unit] {
    completed()
    Ok()
}
assert inferred() is Ok(_)
assert explicit() is Ok(_)
assert interior() is Ok(_)
assert forward_failure() is Err(_)
assert order == 132
assert forward_success() is Ok(_)
assert order == 132
"#);
    execute_both(&program);
    let captured = function(&program, "captured");
    let captures = program.store.functions[captured].captures.bounds(program.store.captures.len()).unwrap();
    let saved = program.store.captures[captures].iter().find(|capture|
        program.store.string(program.store.binding_identities[capture.binding_identity as usize].name).unwrap() == "saved").unwrap();
    let decoder = function_view_decoder(&program.store, captured);
    assert!(decoder.instruction_range.clone().any(|instruction|
        program.store.tags[instruction] == FullTag::ExprParam
            && matches!(decoder.expression_view(instruction).unwrap(), schema::ExprView::Param(slot) if slot == (saved.slot_and_flags & !(1 << 31)) as usize)));
    let parameter = function(&program, "parameter");
    let decoder = function_view_decoder(&program.store, parameter);
    let read = decoder.instruction_range.clone().find(|&instruction|
        program.store.tags[instruction] == FullTag::ExprParam
            && matches!(decoder.expression_view(instruction).unwrap(), schema::ExprView::Param(0))).unwrap();
    let mut wrong_parameter = program.clone();
    wrong_parameter.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&wrong_parameter).unwrap();
    wrong_parameter.store.extra[program.store.data[read].range().start as usize] = 1;
    assert!(matches!(function_view_decoder(&wrong_parameter.store, parameter).expression_view(read).unwrap(), schema::ExprView::Param(1)));
    let error = FullVerifier::verify(&wrong_parameter).unwrap_err();
    assert!(error.message.contains("contract") || error.message.contains("error"), "{error:?}");
    let invalid = without_effect(&program, function(&program, "interior"), Effect::Error);
    let error = FullVerifier::verify(&invalid).unwrap_err();
    assert!(error.message.contains("requires effect error"), "{error:?}");
}

#[test]
fn quantified_registered_result_unit_tail_retains_effect_proof() {
    on_large_stack(|| {
        let program = fixture(r#"
proc inferred_tail(token, file: Path) [fs] {
    let _ = token
    file.write("inferred")
}
proc discarded_write(token, file: Path) [fs, error] {
    let _ = token
    file.write("discarded")
    let _ = 0
}
proc explicit_tail(token, file: Path) [fs, error] -> Result[Unit] {
    let _ = token
    file.write("explicit")
}
proc explicit_return(token, file: Path) [fs] -> Result[Unit] {
    let _ = token
    return file.write("returned")
}
proc ground_tail(file: Path) [fs, error] -> Result[Unit] {
    file.write("ground")
}
tempdir directory {
    let file = fp"{directory}/value"
    inferred_tail(7, file)
    assert file.read_text()? == "inferred"
    inferred_tail("word", file)
    assert file.read_text()? == "inferred"
    let missing_parent = fp"{directory}/missing-parent/value"
    assert inferred_tail(9, missing_parent) is Err(_)
    discarded_write(11, file)
    assert file.read_text()? == "discarded"
    explicit_tail(true, file)
    assert file.read_text()? == "explicit"
    assert explicit_return(false, file) is Ok(_)
    assert file.read_text()? == "returned"
    ground_tail(file)
    assert file.read_text()? == "ground"
}
"#);
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);

        let inferred = function(&program, "inferred_tail");
        let FullCallableContract::Scheme(scheme_id) = program.store.functions[inferred].contract else {
            panic!("unannotated Proc has a quantified contract")
        };
        let scheme = &program.store.schemes[scheme_id.index()];
        let direct = scheme.effects.direct.bounds(program.store.scheme_effects.len()).unwrap();
        let effects = &program.store.scheme_effects[direct.clone()];
        assert!(effects.contains(&Effect::Fs), "{effects:?}");
        assert!(!effects.contains(&Effect::Error), "a Result[Unit] tail is returned as data: {effects:?}");

        let discarded = function(&program, "discarded_write");
        let FullCallableContract::Scheme(discarded_scheme_id) = program.store.functions[discarded].contract else {
            panic!("unannotated discarded-write Proc has a quantified contract")
        };
        let discarded_scheme = &program.store.schemes[discarded_scheme_id.index()];
        let discarded_direct = discarded_scheme.effects.direct.bounds(program.store.scheme_effects.len()).unwrap();
        let discarded_effects = &program.store.scheme_effects[discarded_direct.clone()];
        assert!(discarded_effects.contains(&Effect::Fs), "{discarded_effects:?}");
        let error_offset = discarded_effects.iter().position(|effect| *effect == Effect::Error).unwrap();
        let replacement = Effect::ALL.into_iter().find(|effect| !discarded_effects.contains(effect)).unwrap();
        let mut missing_error_effect = program.clone();
        missing_error_effect.store.scheme_effects[discarded_direct.start + error_offset] = replacement;
        let effective_effects = super::function_effects(&missing_error_effect.store, discarded).unwrap().unwrap();
        assert!(!effective_effects.contains(&Effect::Error), "{effective_effects:?}");
        assert!(FullVerifier::verify(&missing_error_effect).unwrap_err().message.contains("effect error"));

        let explicit_return = function(&program, "explicit_return");
        let FullCallableContract::Scheme(explicit_scheme_id) = program.store.functions[explicit_return].contract else {
            panic!("explicit Result return keeps its quantified contract")
        };
        let explicit_scheme = &program.store.schemes[explicit_scheme_id.index()];
        let explicit_effects = explicit_scheme.effects.direct.bounds(program.store.scheme_effects.len()).unwrap();
        assert!(program.store.scheme_effects[explicit_effects.clone()].contains(&Effect::Fs));
        assert!(!program.store.scheme_effects[explicit_effects].contains(&Effect::Error));
        assert!(FullVerifier::verify(&program).is_ok());

        let restricted = "proc restricted(file: Path) [fs] -> Result[Unit] {\n    file.write(\"rejected\")\n    return Ok()\n}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), restricted);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(
            &parsed.arena,
            restricted,
            CheckOptions::for_execution(),
        );
        assert!(checked.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation)
        }), "{:?}", checked.diagnostics);
    });
}

#[test]
fn quantified_proc_defer_and_failure_return_are_independent() {
    on_large_stack(|| {
        let deferred = r#"
var assertion_messages = 0
proc assertion_message() [] -> Str {
    assertion_messages += 1
    "deferred assertion"
}
proc deferred_success(token) [error] {
    let _ = token
    defer { assert true, assertion_message() }
}
proc deferred_failure(token) [error] {
    let _ = token
    defer { assert false, assertion_message() }
}
assert deferred_success(7) is Ok(_)
assert deferred_success("token") is Ok(_)
assert assertion_messages == 0
let failed_int = deferred_failure(7)
let failed_str = deferred_failure("token")
assert match failed_int {
    Err(AssertionError.Failed {message: message}) => message == "assertion failed: false: deferred assertion"
    _ => false
}
assert match failed_str {
    Err(AssertionError.Failed {message: message}) => message == "assertion failed: false: deferred assertion"
    _ => false
}
assert assertion_messages == 2
"#;
        let returned = r#"
proc returned_failure(token) [error] {
    let _ = token
    return error.fail(f"requested {token}")
}
let failed_int = returned_failure(7)
let failed_str = returned_failure("token")
assert match failed_int {
    Err(problem) => problem.message == "requested 7"
    _ => false
}
assert match failed_str {
    Err(problem) => problem.message == "requested token"
    _ => false
}
"#;
        let deferred_program = fixture(deferred);
        let deferred_result = std::panic::catch_unwind(|| {
            FullVerifier::verify(&deferred_program).unwrap();
            execute_both(&deferred_program);
        });
        let returned_program = fixture(returned);
        let returned_result = std::panic::catch_unwind(|| {
            FullVerifier::verify(&returned_program).unwrap();
            execute_both(&returned_program);
        });
        let failures = [
            ("defer assertion", deferred_result.is_err()),
            ("error.fail return", returned_result.is_err()),
        ].into_iter().filter_map(|(name, failed)| failed.then_some(name)).collect::<Vec<_>>();
        assert!(failures.is_empty(), "generic Proc body witnesses failed: {failures:?}");

        let owner = function(&deferred_program, "deferred_failure");
        let decoder = function_view_decoder(&deferred_program.store, owner);
        let (condition, message) = deferred_program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
            let schema::StmtView::Assert { value, message, .. } = decoder.statement_view(instruction).ok()? else { return None };
            Some((value.raw() as usize, message?.raw() as usize))
        }).unwrap();
        let condition_fact = deferred_program.store.scheme_body_facts.iter().position(|fact| fact.instruction as usize == condition
            && fact.kind() == super::super::schemes::FullSchemeBodyFactKind::Expression).unwrap();
        let message_fact = deferred_program.store.scheme_body_facts.iter().position(|fact| fact.instruction as usize == message
            && fact.kind() == super::super::schemes::FullSchemeBodyFactKind::Expression).unwrap();
        let condition_type = deferred_program.store.scheme_body_facts[condition_fact].ty().unwrap();
        let message_type = deferred_program.store.scheme_body_facts[message_fact].ty().unwrap();
        let mut wrong_condition = deferred_program.clone();
        wrong_condition.store.scheme_body_facts[condition_fact].value = super::super::schemes::FullSchemeBodyFactValue::Expression(message_type);
        assert!(FullVerifier::verify(&wrong_condition).is_err());
        let mut wrong_message = deferred_program.clone();
        wrong_message.store.scheme_body_facts[message_fact].value = super::super::schemes::FullSchemeBodyFactValue::Expression(condition_type);
        assert!(FullVerifier::verify(&wrong_message).is_err());

        let FullCallableContract::Scheme(scheme) = deferred_program.store.functions[owner].contract else { panic!("deferred Proc has a scheme") };
        let direct = deferred_program.store.schemes[scheme.index()].effects.direct.bounds(deferred_program.store.scheme_effects.len()).unwrap();
        let effects = &deferred_program.store.scheme_effects[direct.clone()];
        let error_offset = effects.iter().position(|effect| *effect == Effect::Error).unwrap();
        let replacement = Effect::ALL.into_iter().find(|effect| !effects.contains(effect)).unwrap();
        let mut missing_error = deferred_program.clone();
        missing_error.store.scheme_effects[direct.start + error_offset] = replacement;
        let effective = super::function_effects(&missing_error.store, owner).unwrap().unwrap();
        assert!(!effective.contains(&Effect::Error), "{effective:?}");
        assert!(FullVerifier::verify(&missing_error).unwrap_err().message.contains("effect error"));
    });
}

fn external_stdlib_fixture(source: &str) -> FullProgram {
    let name = "external-stdlib-effects.xsh";
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only_with_linkage(
        name, crate::loader::entry_source_from_text(name, source.to_string()), Vec::new(),
        crate::loader::StdlibLinkage::LinkToPrepared,
    );
    let source_id = sources.files().first().unwrap().id();
    let file = crate::loader::CompactFileUnit::new(name, source_id, parsed);
    assert!(file.parse_diagnostics().is_empty(), "{:?}", file.parse_diagnostics());
    let declarations = Checker::check_compact_declarations(file.program(), &sources, source_id);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    let graph = crate::loader::CompactModuleGraph::from_file_unit(&file, &declarations);
    FullBuilder::build_compact_external_stdlib(file.program(), &declarations, &graph, source, Arc::new(sources), source_id).unwrap()
}

#[test]
fn external_stdlib_calls_authenticate_actual_callee_effects() {
    on_large_stack(|| {
        let program = external_stdlib_fixture("pure style() -> Str { tui.bold() }\nproc verify(file: Path) [fs] -> Result[Unit] { return hash.verify_file(file, sha256: \"00\") }\nproc shadow() [env] -> Result[Str] { return env.get(\"XSH_TEST_CALLEE\") }\nproc plain() [error] -> Unit { assert false }\n");
        FullVerifier::verify(&program).unwrap();
        let denied = without_effect(&program, function(&program, "verify"), Effect::Fs);
        let error = FullVerifier::verify(&denied).unwrap_err();
        assert!(error.message.contains("requires effect fs"), "{error:?}");
        let mut local_shadow = program.clone();
        let shadow = function(&program, "shadow");
        let mut add_string = |text: &str| {
            let id = u32::try_from(local_shadow.store.strings.len() + 1).unwrap();
            let start = u32::try_from(local_shadow.store.string_bytes.len()).unwrap();
            local_shadow.store.string_bytes.extend_from_slice(text.as_bytes());
            local_shadow.store.strings.push(IrRange::new(start, u32::try_from(text.len()).unwrap()));
            id
        };
        let namespace = add_string("<xsh-stdlib:tui>");
        let member = add_string("bold");
        local_shadow.store.functions[shadow].name = member;
        local_shadow.store.function_metadata[shadow].owner = namespace;
        let error = FullVerifier::verify(&local_shadow).unwrap_err();
        assert!(error.message.contains("script implementation kind differs"), "{error:?}");
        let mut finite_shadow = program.clone();
        let start = u32::try_from(finite_shadow.store.string_bytes.len()).unwrap();
        finite_shadow.store.string_bytes.extend_from_slice(b"<xsh-stdlib:hash>verify_file");
        let namespace = u32::try_from(finite_shadow.store.strings.len() + 1).unwrap();
        finite_shadow.store.strings.push(IrRange::new(start, 17));
        let member = u32::try_from(finite_shadow.store.strings.len() + 1).unwrap();
        finite_shadow.store.strings.push(IrRange::new(start + 17, 11));
        finite_shadow.store.functions[shadow].name = member;
        finite_shadow.store.function_metadata[shadow].owner = namespace;
        let error = FullVerifier::verify(&finite_shadow).unwrap_err();
        assert!(error.message.contains("script implementation effects exceed"), "{error:?}");
        let mut plain_shadow = finite_shadow.clone();
        plain_shadow.store.functions[shadow] = program.store.functions[shadow];
        plain_shadow.store.function_metadata[shadow] = program.store.function_metadata[shadow];
        let plain = function(&program, "plain");
        plain_shadow.store.functions[plain].name = member;
        plain_shadow.store.function_metadata[plain].owner = namespace;
        let error = FullVerifier::verify(&plain_shadow).unwrap_err();
        assert!(error.message.contains("script implementation effects exceed"), "{error:?}");
        program.symbol_owner().with_current(|| {
            let owner = function(&program, "style");
            let call = program.store.function_instruction_range(owner).unwrap()
                .find(|instruction| program.store.tags[*instruction] == FullTag::ExprExternalCall).unwrap();
            let words = program.store.data[call].range().bounds(program.store.extra.len()).unwrap();
            for (namespace, member) in [
                ("<xsh-stdlib:hash>", "verify_file"),
                ("<xsh-stdlib:tui>", "missing"),
                ("user-module", "bold"),
            ] {
                let mut forged = program.clone();
                forged.store.extra[words.start] = Name::intern(namespace).symbol().raw();
                forged.store.extra[words.start + 1] = Name::intern(member).symbol().raw();
                let error = FullVerifier::verify(&forged).unwrap_err();
                assert!(error.message.contains("pure executable body contains an impure operation"), "{error:?}");
            }
        });
    });
}

#[cfg(target_os = "linux")]
#[test]
fn prepared_uptime_implementation_requires_filesystem_permission() {
    on_large_stack(|| {
        let program = fixture("proc uptime() [fs] -> Result[Int] { return unix.uptime_seconds() }\n");
        FullVerifier::verify(&program).unwrap();
        let denied = without_effect(&program, function(&program, "uptime"), Effect::Fs);
        let error = FullVerifier::verify(&denied).unwrap_err();
        assert!(error.message.contains("requires effect fs"), "{error:?}");
        let source = "proc uptime() [process] -> Result[Int] { return unix.uptime_seconds() }\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation)), "{:?}", checked.diagnostics);
    });
}



#[test]
fn process_handle_cancel_preserves_compacted_named_defaults() {
    let mut program = fixture(r#"
proc close(handle: ProcessHandle) [process] -> Result[Unit] {
    handle.cancel(kill_after: 0ms)
}
proc signal(handle: ProcessHandle) [process] -> Result[Unit] {
    handle.cancel(signal: "TERM")
}
proc both(handle: ProcessHandle) [process] -> Result[Unit] {
    handle.cancel(signal: "TERM", kill_after: 0ms)
}
proc defaults(handle: ProcessHandle) [process] -> Result[Unit] {
    handle.cancel()
}
pure wrong() -> Bool { true }
"#);
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    let owner = function(&program, "close");
    let decoder = function_view_decoder(&program.store, owner);
    let method = program.store.function_instruction_range(owner).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprMethod).unwrap();
    let schema::ExprView::Method { mut args, .. } = decoder.expression_view(method).unwrap() else { panic!("method") };
    assert_eq!(args.len(), 1);
    let argument = args.next().unwrap().unwrap().raw() as usize;
    assert!(program.store.function_instruction_range(owner).unwrap().contains(&argument));
    let durations = program.store.function_instruction_range(owner).unwrap().filter(|&instruction|
        matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Duration(_)
            | schema::ExprView::PreparedConstant(PreparedConstantValue(LoweredValue::Duration(_), _))))).collect::<Vec<_>>();
    assert_eq!(durations.len(), 1);
    let duration = durations[0];
    let boolean = program.store.function_instruction_range(function(&program, "wrong")).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
    let mut invalid = program.clone();
    invalid.store.tags[duration] = FullTag::ExprBool;
    invalid.store.data[duration] = program.store.data[boolean];
    let error = FullVerifier::verify(&invalid).unwrap_err();
    assert!(error.message.contains("ground method arguments do not match a registered contract"), "{error:?}");
}

#[test]
fn process_port_rows_preserve_pipeline_predicate_categories() {
    let actual = fixture(r#"
type PortProcess = {
    pid: Int, parent_pid: Int, command: Str, argv: Str, argv0: Str,
    user: Str, uid: Int, protocol: Str, local_address: Str,
    local_port: Int, local: Str, remote_address: Str, remote_port: Int,
    remote: Str, state: Str, fd: Int, inode: Int,
}
let rows: List[PortProcess] = process.port(1)? |> where .pid == 1
let count = rows |> count()
if count == 0 {
    print false
} else {
    let row = rows[0]
    print ${row.pid == 1}
}
"#);
    assert!(actual.store.tags.contains(&FullTag::BoolLiteralCompareSlot));
    let mut program = fixture(r#"
proc flag() [] -> Bool { true }
let count: Int = if flag() { 7 } else { 8 }
let alternate: Int = 9
if count == 7 {
    assert true
} else {
    assert false
}
"#);
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    let integer = (0..program.store.driver_steps.len()).flat_map(|step| program.store.driver_instruction_range(step).unwrap()).find(|&instruction| {
        if program.store.tags[instruction].kind() != schema::InstructionKind::Expr { return false; }
        let decoder = driver_projection_decoder(&program, instruction);
        matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Int(7)
            | schema::ExprView::PreparedConstant(PreparedConstantValue(LoweredValue::Int(7), _))))
    }).unwrap();
    let boolean = program.store.function_instruction_range(function(&program, "flag")).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
    let mut wrong_writer = program.clone();
    wrong_writer.store.tags[integer] = FullTag::ExprBool;
    wrong_writer.store.data[integer] = program.store.data[boolean];
    let error = FullVerifier::verify(&wrong_writer).unwrap_err();
    assert!(error.message.contains("producer") || error.message.contains("annotation"), "{error:?}");
    let predicate = program.store.tags.iter().position(|tag| *tag == FullTag::BoolLiteralCompareSlot).unwrap();
    let decoder = driver_projection_decoder(&program, predicate);
    let step = program.store.driver_steps[driver_owner_index(decoder.owner).unwrap()];
    let read = step.slots.bounds(program.store.driver_slots.len()).unwrap()
        .find(|&index| program.store.string(program.store.driver_slots[index].name).unwrap() == "count").unwrap();
    let alternate = program.store.binding_identities.iter().position(|identity|
        program.store.string(identity.name).unwrap() == "alternate").unwrap();
    let mut wrong_owner = program.clone();
    assert_ne!(wrong_owner.store.driver_slots[read].binding_identity, alternate as u32);
    wrong_owner.store.driver_slots[read].binding_identity = alternate as u32;
    let error = FullVerifier::verify(&wrong_owner).unwrap_err();
    assert!(error.message.contains("identity") || error.message.contains("identities"), "{error:?}");
}

#[test]
fn parallel_worker_callbacks_preserve_driver_bindings() {
    let actual = fixture(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/runtime/stack-depth/par-map-worker-recursion.xsh")));
    assert!(actual.store.tags.contains(&FullTag::ExprPipeline));
    let mut program = fixture(r#"
var order = 0
proc start() [] -> Int { order = order * 10 + 1; 0 }
proc end() [] -> Int { order = order * 10 + 2; 3 }
pure wrong() -> Bool { true }
let rows = [0] |> range(end: end(), start: start())
assert rows.collect() == [0, 1, 2]
assert order == 21
let repeated = [0] |> range(0, 3) |> range(0, 4)
assert repeated.collect() == [0, 1, 2, 3]
let values = rows |> par-map(jobs: 2) { |item| item + 1 }
assert values == [1, 2, 3]
"#);
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    let mut ranges = Vec::new();
    for instruction in 0..program.store.tags.len() {
        if program.store.tags[instruction] != FullTag::ExprPipeline { continue; }
        let decoder = driver_projection_decoder(&program, instruction);
        let schema::ExprView::ListPipeline { mut stages, .. } = decoder.expression_view(instruction).unwrap() else { unreachable!() };
        for stage in &mut stages {
            if let StageView::Range { start, end } = stage.unwrap() {
                let bounds = indexed_configured_range_bounds_with(&decoder, start.raw(), end.raw(), &mut |_| Ok(())).unwrap().unwrap();
                ranges.push((end.raw() as usize, bounds.end_subject as usize, bounds.end_slot));
            }
        }
    }
    assert_eq!(ranges.len(), 3);
    let boolean = program.store.function_instruction_range(function(&program, "wrong")).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
    for (position, expected) in [(1, 3), (2, 4)] {
        let mut wrong_domain = program.clone();
        let subject = ranges[position].1;
        let actual = match driver_projection_decoder(&program, subject).expression_view(subject).unwrap() {
            schema::ExprView::Int(value) => value,
            schema::ExprView::PreparedConstant(PreparedConstantValue(LoweredValue::Int(value), _)) => *value,
            _ => panic!("range bound is not the original integer producer"),
        };
        assert_eq!(actual, expected);
        wrong_domain.store.tags[subject] = FullTag::ExprBool;
        wrong_domain.store.data[subject] = program.store.data[boolean];
        let error = FullVerifier::verify(&wrong_domain).unwrap_err();
        assert!(error.message.contains("producer") || error.message.contains("contract"), "{error:?}");
    }
    let mut wrong_stage = program.clone();
    assert_ne!(ranges[1].2, ranges[2].2);
    wrong_stage.store.data[ranges[1].0] = program.store.data[ranges[2].0];
    let error = FullVerifier::verify(&wrong_stage).unwrap_err();
    assert!(error.message.contains("binding") || error.message.contains("slot"), "{error:?}");
}



#[test]
fn checked_field_reads_preserve_terminating_guards() {
    let program = fixture(r#"
type Index = {value: Int?}
type Label = {name: Str?}
pure add(value: Int) -> Int { value + 1 }
pure count(rows: List[Index]) -> Int {
    var total = 0
    for row in rows {
        if row.value == null { continue }
        total += add(row.value)
    }
    total
}
pure names(rows: List[Label]) -> List[Str] {
    let result: List[Str] = collect {
        for row in rows {
            if row.name == null or row.name == "" { continue }
            yield row.name
        }
    }
    result
}
assert count([{value: null}, {value: 4}]) == 5
assert names([{name: null}, {name: ""}, {name: "kept"}]) == ["kept"]
"#);
    execute_both(&program);
}

#[test]
fn checked_mutable_reads_preserve_terminating_guards() {
    let program = fixture(r#"
type Row = {root: Str, point: Str}
pure choose(flag: Bool) -> Str {
    var selected: Row? = null
    if flag { selected = {root: "/", point: "chosen"} }
    if selected == null { return "none" }
    let chosen: Row = selected
    chosen.point
}
pure choose_int(flag: Bool) -> Int {
    var value: Int? = null
    if flag { value = 7 }
    if value == null { return 0 }
    let narrowed: Int = value
    narrowed
}
pure choose_after_write(value: Row?, flag: Bool) -> Str {
    var selected: Row? = null
    if { selected = value; flag } { return "skip" } else if selected == null { return "none" }
    let chosen: Row = selected
    chosen.point
}
assert choose_int(true) == 7
assert choose_int(false) == 0
assert choose_after_write({root: "/", point: "kept"}, false) == "kept"
assert choose_after_write(null, false) == "none"
assert choose(true) == "chosen"
assert choose(false) == "none"
"#);
    let mut program = program;
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    for name in ["choose", "choose_int"] {
        let owner = function(&program, name);
        let decoder = function_view_decoder(&program.store, owner);
        assert!(program.store.function_instruction_range(owner).unwrap().any(|instruction| {
            matches!(decoder.expression_view(instruction), Ok(schema::ExprView::CheckedValue { value, check: ValueCheckView::Ground(check), .. })
                if check.schema.is_none() && (match check.ty.materialize().unwrap() { Type::Record(_) => name == "choose", Type::Int => name == "choose_int", _ => false })
                    && matches!(decoder.expression_view(value.raw() as usize), Ok(schema::ExprView::Param(_))))
        }));
    }
}

#[test]
fn successful_assertions_narrow_optional_binding_reads() {
    let program = fixture(r#"
proc assert_ordered(text: Str, fragments: List[Str]) [error] {
  var start = 0
  for fragment in fragments {
    let offset = text.find(fragment, start)
    assert offset != null, f"missing {fragment} after byte {start}: {text}"
    start = offset + fragment.byte_len()
  }
}
proc if_ordered(text: Str, fragments: List[Str]) [error] {
  var start = 0
  for fragment in fragments {
    let offset = text.find(fragment, start)
    if offset == null { return }
    start = offset + fragment.byte_len()
  }
}
type Row = {value: Int?}
proc read_path(row: Row) [error] -> Int {
  assert row.value != null
  let checked: Int = row.value
  checked
}
proc exercise() [error] -> Result[Unit] {
  assert_ordered("alpha beta", ["alpha", "beta"])?
  if_ordered("alpha beta", ["alpha", "beta"])?
  assert read_path({value: 7}) == 7
  Ok()
}
exercise()?
var continued = false
let failure: Result[Unit] = try {
  assert false
  continued = true
}
assert !continued
assert failure is Err(_)
"#);
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
}

#[test]
fn assertion_narrowing_requires_its_binding_and_precedes_the_read() {
    let program = fixture(r#"
type Callback = pure(value: Int) -> Int
proc narrow(value: Callback?, other: Callback?) [error] -> Callback {
  assert value != null
  let checked: Callback = value
  checked
}
"#);
    FullVerifier::verify(&program).unwrap();
    let owner = function(&program, "narrow");
    let range = program.store.function_instruction_range(owner).unwrap();
    let decoder = function_view_decoder(&program.store, owner);
    let assertion = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::StmtAssert).unwrap();
    let condition = match decoder.statement_view(assertion).unwrap() {
        schema::StmtView::Assert { value, .. } => value.raw() as usize,
        _ => unreachable!(),
    };
    let schema::ExprView::Binary { op: BinaryOp::Ne, left, right, .. } = decoder.expression_view(condition).unwrap() else { panic!("assert condition is not a null comparison") };
    assert!(matches!(decoder.expression_view(left.raw() as usize), Ok(schema::ExprView::Param(0))));
    assert!(matches!(decoder.expression_view(right.raw() as usize), Ok(schema::ExprView::Null)));
    let condition_slot = program.store.data[left.raw() as usize].range().start as usize;
    let checked = range.clone().find(|&instruction| matches!(decoder.statement_view(instruction),
        Ok(schema::StmtView::Let { slot, .. }) if slot >= program.store.functions[owner].params.len as usize)).unwrap();
    let checked_input = match decoder.statement_view(checked).unwrap() {
        schema::StmtView::Let { value, .. } => match decoder.expression_view(value.raw() as usize).unwrap() {
            schema::ExprView::CheckedValue { value, .. } => value.raw() as usize,
            _ => panic!("callable annotation is not checked"),
        },
        _ => unreachable!(),
    };
    assert!(matches!(decoder.expression_view(checked_input), Ok(schema::ExprView::Param(0))));
    let block = program.store.blocks.iter().enumerate().find_map(|(index, block)| {
        if block.flags & super::super::BLOCK_STATEMENTS == 0 { return None; }
        let words = block.instructions.bounds(program.store.extra.len())?;
        let statements = &program.store.extra[words.start + 1..words.end];
        (statements.contains(&(assertion as u32)) && statements.contains(&(checked as u32))).then_some(index)
    }).unwrap();
    let words = program.store.blocks[block].instructions.bounds(program.store.extra.len()).unwrap();
    let asserted_position = (words.start + 1..words.end).find(|&position| program.store.extra[position] == assertion as u32).unwrap();
    let checked_position = (words.start + 1..words.end).find(|&position| program.store.extra[position] == checked as u32).unwrap();

    let mut wrong_guard = program.clone();
    wrong_guard.store.extra[condition_slot] = 1;
    assert!(matches!(function_view_decoder(&wrong_guard.store, owner).expression_view(condition).unwrap(),
        schema::ExprView::Binary { left, .. } if matches!(function_view_decoder(&wrong_guard.store, owner).expression_view(left.raw() as usize), Ok(schema::ExprView::Param(1)))));
    assert!(matches!(function_view_decoder(&wrong_guard.store, owner).expression_view(checked_input), Ok(schema::ExprView::Param(0))));
    let error = FullVerifier::verify(&wrong_guard).unwrap_err();
    assert!(error.message.contains("non-null branch proof") || error.message.contains("checked value input disagrees"), "{error:?}");

    let mut late_assertion = program.clone();
    late_assertion.store.extra.swap(asserted_position, checked_position);
    let error = FullVerifier::verify(&late_assertion).unwrap_err();
    assert!(error.message.contains("non-null branch proof") || error.message.contains("checked value input disagrees"), "{error:?}");

    let source = r#"
proc stale(value: Int?) [error] -> Int {
  var current: Int? = value
  assert current != null
  current = null
  let checked: Int = current
  checked
}
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)));
}

#[test]
fn checked_prefix_guards_reject_fallthrough_writes() {
    for (index, guard) in [
        "if selected == null { return \"none\" } else if { other = null; flag } { return \"skip\" }",
        "if selected == null { return \"none\" } else if flag { return \"skip\" } else { other = null }",
        "if selected != null { other = null } else { return \"none\" }",
    ].into_iter().enumerate() {
        let source = format!("type Row = {{point: Str}}\npure choose(value: Row?, flag: Bool) -> Str {{\nvar selected: Row? = value\nvar other: Row? = null\n{guard}\nlet chosen = selected\nchosen.point\n}}\nassert choose({{point: \"kept\"}}, false) == \"kept\"\nassert choose(null, false) == \"none\"\n");
        let program = fixture(&source);
        execute_both(&program);
        let owner = function(&program, "choose");
        let decoder = function_view_decoder(&program.store, owner);
        let range = program.store.function_instruction_range(owner).unwrap();
        let selected = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::CheckedValue { value, check: ValueCheckView::Ground(check), .. }) if check.schema.is_none() && matches!(check.ty.materialize().unwrap(), Type::Record(_)) => match decoder.expression_view(value.raw() as usize).unwrap() {
                schema::ExprView::Param(slot) => Some(slot),
                _ => None,
            },
            _ => None,
        }).unwrap();
        assert!(selected >= program.store.functions[owner].params.len as usize);
        let (condition, is_boolean) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::If { mut branches, .. }) => Some((branches.next().unwrap().unwrap().0.raw() as usize, false)),
            Ok(schema::StmtView::IfBool { mut branches, .. }) => Some((branches.next().unwrap().unwrap().0.raw() as usize, true)),
            _ => None,
        }).unwrap();
        assert!(decoder.checked_slot_null_condition_proves_nonnull_with(condition, index == 2, is_boolean, selected, |_| Ok(())).unwrap());
        let (assignment, original) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Assign { slot, value, .. }) if matches!(decoder.expression_view(value.raw() as usize), Ok(schema::ExprView::Null)) => Some((instruction, slot)),
            _ => None,
        }).unwrap();
        assert_ne!(original, selected);
        let mut malformed = program.clone();
        malformed.store.checked_expr_types.retain(|fact| fact.is_contract());
        malformed.store.extra[program.store.data[assignment].range().start as usize] = selected as u32;
        assert!(matches!(function_view_decoder(&malformed.store, owner).statement_view(assignment).unwrap(), schema::StmtView::Assign { slot, .. } if slot == selected));
        let error = FullVerifier::verify(&malformed).unwrap_err();
        assert!(error.message.contains("checked value input disagrees") || error.message.contains("non-null branch proof"), "{error:?}");
    }
}

#[test]
fn checked_guarded_operands_preserve_evaluation_order() {
    let program = fixture(r#"
type Row = {value: Int}
var shared: Row? = {value: 7}
proc clear_shared() [] -> Int { shared = null; 0 }
proc read_before_clear() [] -> Int {
    if shared != null {
        let result = shared.value + clear_shared()
        result
    } else { 0 }
}
assert read_before_clear() == 7
assert shared == null
"#);
    execute_both(&program);
    let owner = function(&program, "read_before_clear");
    let setter = function(&program, "clear_shared");
    let capture = program.store.captures[program.store.functions[owner].captures.bounds(program.store.captures.len()).unwrap()].iter()
        .find(|capture| capture.slot_and_flags & (1 << 31) != 0).unwrap();
    assert!(program.store.captures[program.store.functions[setter].captures.bounds(program.store.captures.len()).unwrap()].iter()
        .any(|other| other.slot_and_flags & (1 << 31) != 0 && other.binding_identity == capture.binding_identity));
    let decoder = function_view_decoder(&program.store, owner);
    let (binary, left, right) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
        let schema::ExprView::Binary { op: BinaryOp::Add, left, right, .. } = decoder.expression_view(instruction).ok()? else { return None };
        matches!(decoder.expression_view(right.raw() as usize), Ok(schema::ExprView::Call { function, .. }) if function.id.index() == setter)
            .then_some((instruction, left.raw() as usize, right.raw() as usize))
    }).unwrap();
    assert!(matches!(decoder.expression_view(left).unwrap(), schema::ExprView::Field { .. }));
    let mut malformed = program.clone();
    malformed.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&malformed).unwrap();
    let payload = program.store.data[binary].range().start as usize;
    assert_eq!(malformed.store.extra[payload + 1], left as u32);
    assert_eq!(malformed.store.extra[payload + 2], right as u32);
    malformed.store.extra.swap(payload + 1, payload + 2);
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(binary).unwrap(), schema::ExprView::Binary { left: actual_left, right: actual_right, .. } if actual_left.raw() as usize == right && actual_right.raw() as usize == left));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("non-null branch proof") || error.message.contains("checked value input disagrees"), "{error:?}");
}

#[test]
fn checked_guarded_list_operands_preserve_evaluation_order() {
    let program = fixture(r#"
type Row = {value: Int}
var shared: Row? = {value: 7}
proc clear_shared() [] -> Int { shared = null; 0 }
proc read_before_clear() [] -> List[Int] {
    if shared != null { [shared.value, clear_shared()] } else { [] }
}
assert read_before_clear() == [7, 0]
assert shared == null
"#);
    execute_both(&program);
    let owner = function(&program, "read_before_clear");
    let setter = function(&program, "clear_shared");
    let decoder = function_view_decoder(&program.store, owner);
    let (list, block, left, right) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
        let schema::ExprView::List(mut items) = decoder.expression_view(instruction).ok()? else { return None };
        if items.len() != 2 { return None; }
        let block = items.block_identity_for_test();
        let left = items.next()?.ok()?.raw() as usize;
        let right = items.next()?.ok()?.raw() as usize;
        matches!(decoder.expression_view(right), Ok(schema::ExprView::Call { function, .. }) if function.id.index() == setter)
            .then_some((instruction, block, left, right))
    }).unwrap();
    assert!(matches!(decoder.expression_view(left).unwrap(), schema::ExprView::Field { .. }));
    let words = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
    assert_eq!(&program.store.extra[words.clone()], &[2, left as u32, right as u32]);
    let mut malformed = program.clone();
    malformed.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&malformed).unwrap();
    malformed.store.extra.swap(words.start + 1, words.start + 2);
    let decoder = function_view_decoder(&malformed.store, owner);
    let schema::ExprView::List(mut items) = decoder.expression_view(list).unwrap() else { panic!("expected list"); };
    assert_eq!(items.next().unwrap().unwrap().raw() as usize, right);
    assert_eq!(items.next().unwrap().unwrap().raw() as usize, left);
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("non-null branch proof") || error.message.contains("checked value input disagrees"), "{error:?}");
}

#[test]
fn checked_guarded_interpolations_and_tags_preserve_evaluation_order() {
    for (declaration, result, expression, empty, expected) in [
        ("", "Str", r#"f"{shared.value}:{clear_shared()}""#, r#""""#, r#""7:0""#),
        ("enum Triple { Values(Int, Int, Int) }", "Triple", "Values(0, shared.value, clear_shared())", "Values(0, 0, 0)", "Values(0, 7, 0)"),
    ] {
        let source = format!("type Row = {{value: Int}}\n{declaration}\nvar shared: Row? = {{value: 7}}\nproc clear_shared() [] -> Int {{ shared = null; 0 }}\nproc read_before_clear() [] -> {result} {{ if shared != null {{ {expression} }} else {{ {empty} }} }}\nassert read_before_clear() == {expected}\nassert shared == null\n");
        let program = fixture(&source);
        execute_both(&program);
        let owner = function(&program, "read_before_clear");
        let setter = function(&program, "clear_shared");
        let decoder = function_view_decoder(&program.store, owner);
        let (block, left, right, format) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
            let (block, values, format) = match decoder.expression_view(instruction).ok()? {
                schema::ExprView::FmtString(parts) => {
                    let block = parts.block_identity_for_test();
                    let values = parts.filter_map(|part| match part.ok()? { decode::FmtPartView::Expr(value, ..) => Some(value.raw() as usize), _ => None }).collect::<Vec<_>>();
                    (block, values, true)
                }
                schema::ExprView::Tag { fields, .. } => {
                    let block = fields.block_identity_for_test();
                    (block, fields.map(|value| value.unwrap().raw() as usize).collect::<Vec<_>>(), false)
                }
                _ => return None,
            };
            let left = *values.iter().find(|&&value| matches!(decoder.expression_view(value), Ok(schema::ExprView::Field { .. })))?;
            let right = *values.iter().find(|&&value| matches!(decoder.expression_view(value), Ok(schema::ExprView::Call { function, .. }) if function.id.index() == setter))?;
            Some((block, left, right, format))
        }).unwrap();
        let words = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
        let offsets = if format {
            let entries = &program.store.extra[words.clone()];
            let left_offset = entries.windows(2).position(|entry| entry == [1, left as u32]).unwrap() + 1;
            let right_offset = entries.windows(2).position(|entry| entry == [1, right as u32]).unwrap() + 1;
            (words.start + left_offset, words.start + right_offset)
        } else {
            assert_eq!(&program.store.extra[words.clone()][2..], &[left as u32, right as u32]);
            (words.start + 2, words.start + 3)
        };
        let mut malformed = program.clone();
        malformed.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&malformed).unwrap();
        malformed.store.extra.swap(offsets.0, offsets.1);
        assert_eq!(malformed.store.extra[offsets.0], right as u32);
        assert_eq!(malformed.store.extra[offsets.1], left as u32);
        let error = FullVerifier::verify(&malformed).unwrap_err();
        assert!(error.message.contains("non-null branch proof") || error.message.contains("checked value input disagrees"), "{error:?}");
    }
}

#[test]
fn checked_guarded_branch_conditions_preserve_evaluation_order() {
    let program = fixture(r#"
type Row = {value: Int}
var shared: Row? = {value: 7}
proc clear_shared() [] -> Int { shared = null; 0 }
proc read_before_clear() [] -> Int {
    if shared != null {
        let result = if shared.value == 1 { 1 } else if clear_shared() == 0 { 2 } else { 3 }
        result
    } else { 0 }
}
assert read_before_clear() == 2
assert shared == null
"#);
    execute_both(&program);
    let owner = function(&program, "read_before_clear");
    let setter = function(&program, "clear_shared");
    let decoder = function_view_decoder(&program.store, owner);
    let mut field_condition = None;
    let mut call_condition = None;
    for instruction in program.store.function_instruction_range(owner).unwrap() {
        let Ok(schema::ExprView::Binary { op: BinaryOp::Eq, left, .. }) = decoder.expression_view(instruction) else { continue };
        match decoder.expression_view(left.raw() as usize).unwrap() {
            schema::ExprView::Field { .. } => field_condition = Some((instruction, left.raw() as usize)),
            schema::ExprView::Call { function, .. } if function.id.index() == setter => call_condition = Some((instruction, left.raw() as usize)),
            _ => {}
        }
    }
    let (field_condition, field) = field_condition.unwrap();
    let (call_condition, call) = call_condition.unwrap();
    let field_word = program.store.data[field_condition].range().start as usize + 1;
    let call_word = program.store.data[call_condition].range().start as usize + 1;
    assert_eq!(program.store.extra[field_word], field as u32);
    assert_eq!(program.store.extra[call_word], call as u32);
    let mut malformed = program.clone();
    malformed.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&malformed).unwrap();
    malformed.store.extra.swap(field_word, call_word);
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(call_condition).unwrap(), schema::ExprView::Binary { left, .. } if left.raw() as usize == field));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("non-null branch proof") || error.message.contains("checked value input disagrees"), "{error:?}");
}

#[test]
fn checked_guarded_reads_keep_repeated_uses_bounded() {
    for count in [16, 32, 64, 128] {
        let mut body = String::new();
        for _ in 0..count { body.push_str("total += row.value\n"); }
        let source = format!("type Row = {{value: Int?}}\npure sum(row: Row) -> Int {{\nif row.value == null {{ return 0 }}\nvar total = 0\n{body}total\n}}\nassert sum({{value: 2}}) == {}\nassert sum({{value: null}}) == 0\n", count * 2);
        let mut program = fixture(&source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
    }
}

#[test]
fn checked_optional_iteration_preserves_prefix_guards() {
    let program = fixture(r#"
pure sum(values: List[Int]?) -> Int {
    if values == null { return 0 }
    var total = 0
    for value in values { total += value }
    total
}
assert sum([2, 3]) == 5
assert sum(null) == 0
"#);
    execute_both(&program);
}

#[test]
fn checked_pattern_optional_iteration_preserves_prefix_guards() {
    let program = fixture(r#"
enum Action { Delete(List[Int]?), Skip }
pure sum(action: Action) -> Int {
    match action {
        Delete(keys) => {
            if keys == null { return 0 }
            var total = 0
            for key in keys { total += key }
            total
        }
        Skip => 0
    }
}
assert sum(Delete([2, 3])) == 5
assert sum(Delete(null)) == 0
assert sum(Skip) == 0
pure count(value: List[Int]?) -> Int {
    if value == null { return 0 }
    value.len()
}
assert count([2, 3]) == 2
assert count(null) == 0
"#);
    execute_both(&program);
}

#[test]
fn checked_captured_explicit_errors_preserve_transfer_contracts() {
    let program = fixture(r#"
error Stopped = Failed(message: Str)
let result: Result[Unit, Stopped] = try { Err(Stopped.Failed(message: "stopped"))? }
assert result == Err(Stopped.Failed(message: "stopped"))
pure fail_value() -> Result[Unit] { error.fail("stopped") }
assert match fail_value() { Err(_) => true; Ok(_) => false }
"#);
    execute_both(&program);
    let (step, transfer) = (0..program.store.driver_steps.len()).find_map(|step| {
        let decoder = verify_schemes::decoder_for_driver_step(&program.store, step).unwrap();
        decoder.instruction_range.clone().find(|&instruction|
            program.store.try_operand(instruction).unwrap().is_some_and(|value|
                matches!(decoder.expression_view(value).unwrap(), schema::ExprView::Err { .. }))).map(|instruction| (step, instruction))
    }).unwrap();
    let decoder = verify_schemes::decoder_for_driver_step(&program.store, step).unwrap();
    let operand = program.store.try_operand(transfer).unwrap().unwrap();
    let schema::ExprView::Err { value, cause } = decoder.expression_view(operand).unwrap() else { panic!("direct error operand") };
    let payload = value.raw() as usize;
    assert!(cause.is_none());
    assert_eq!(program.store.tags[payload], FullTag::ExprMatch);
    let message = decoder.instruction_range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprStr).unwrap();
    assert!(matches!(decoder.expression_view(message).unwrap(), schema::ExprView::Str(_)));
    let boolean = program.store.tags.iter().position(|tag| *tag == FullTag::ExprBool).unwrap();
    let mut malformed = program.clone();
    malformed.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&malformed).unwrap();
    malformed.store.tags[message] = FullTag::ExprBool;
    malformed.store.data[message] = program.store.data[boolean];
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("contract") || error.message.contains("error") || error.message.contains("Error"), "{error:?}");
    let mut program = fixture(r#"
error First = Failed(message: Str)
error Other = Failed(message: Str)
pure transfer(value: First, other: Other) -> Result[Unit, First] {
    Err(value)?
    Ok()
}
assert match transfer(First.Failed(message: "first"), Other.Failed(message: "other")) { Err(_) => true; Ok(_) => false }
"#);
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    let owner = function(&program, "transfer");
    let decoder = function_view_decoder(&program.store, owner);
    let (transfer, payload) = decoder.instruction_range.clone().find_map(|instruction| {
        let operand = program.store.try_operand(instruction).unwrap()?;
        let schema::ExprView::Err { value, cause } = decoder.expression_view(operand).unwrap() else { return None };
        assert!(cause.is_none());
        Some((instruction, value.raw() as usize))
    }).unwrap();
    assert_eq!(program.store.tags[transfer], FullTag::ExprTry);
    assert!(matches!(decoder.expression_view(payload).unwrap(), schema::ExprView::Param(0)));
    let mut malformed = program.clone();
    malformed.store.extra[program.store.data[payload].range().start as usize] = 1;
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(payload).unwrap(), schema::ExprView::Param(1)));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("error") || error.message.contains("contract"), "{error:?}");

}

#[test]
fn checked_named_error_transfers_preserve_binding_sources_and_causes() {
    let source = r#"
error Stopped = Failed(message: Str)
pure capture(value: Stopped, cause: Error, spare: Bool) -> Result[Unit, Stopped] {
    let result: Result[Unit, Stopped] = try { Err(value, cause: cause)? }
    return result
}
assert match capture(Stopped.Failed(message: "stopped"), Stopped.Failed(message: "cause"), true) { Err(_) => true; Ok(_) => false }
"#;
    for tail in ["return result", "result"] {
        let mut program = fixture(&source.replace("return result", tail));
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        let owner = function(&program, "capture");
        let (transfer, first, second) = {
            let decoder = function_view_decoder(&program.store, owner);
            let transfer = decoder.instruction_range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprTry).unwrap();
            let schema::ExprView::Try { value, .. } = decoder.expression_view(transfer).unwrap() else { unreachable!() };
            assert!(matches!(decoder.expression_view(value.raw() as usize).unwrap(), schema::ExprView::MatchExpr { .. }));
            let first = decoder.argument_binding_match(value.raw() as usize).unwrap().unwrap();
            let second = decoder.argument_binding_match(first.result).unwrap().unwrap();
            (transfer, first, second)
        };
        let facts = program.store.functions[owner].slot_type_facts;
        for slot in [first.slot, second.slot] {
            if slot < facts.len as usize { program.store.slot_type_facts[facts.start as usize + slot] = IR_NONE; }
        }
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let decoder = function_view_decoder(&program.store, owner);
        assert!(matches!(decoder.expression_view(first.source).unwrap(), schema::ExprView::Param(0)));
        assert!(matches!(decoder.expression_view(second.source).unwrap(), schema::ExprView::Param(1)));
        let schema::ExprView::Err { value, cause: Some(cause) } = decoder.expression_view(second.result).unwrap() else { panic!("terminal error") };
        assert!(matches!(decoder.expression_view(value.raw() as usize).unwrap(), schema::ExprView::Param(slot) if slot == first.slot));
        assert!(matches!(decoder.expression_view(cause.raw() as usize).unwrap(), schema::ExprView::Param(slot) if slot == second.slot));
        assert!(!indexed_expr_can_complete(&decoder, transfer, 0).unwrap());
        for (source, replacement) in [
            (first.source, 2),
            (second.source, 2),
            (value.raw() as usize, 2),
            (cause.raw() as usize, 2),
            (value.raw() as usize, second.slot),
        ] {
            let mut malformed = program.clone();
            malformed.store.extra[program.store.data[source].range().start as usize] = replacement as u32;
            assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(source).unwrap(), schema::ExprView::Param(slot) if slot == replacement));
            let error = FullVerifier::verify(&malformed).err().unwrap_or_else(|| panic!("accepted {tail} error binding retarget source={source}, replacement={replacement}, wrapper_sources=({}, {}), leaves=({}, {})", first.source, second.source, value.raw(), cause.raw()));
            assert!(error.message.contains("error") || error.message.contains("Error") || error.message.contains("contract"), "{error:?}");
        }
    }
    let ordered = fixture(r#"
error Stopped = Failed(message: Str)
var order = 0
proc payload() [] -> Stopped { order = order * 10 + 1; Stopped.Failed(message: "payload") }
proc cause() [] -> Stopped { order = order * 10 + 2; Stopped.Failed(message: "cause") }
let result: Result[Unit, Stopped] = try { Err(cause: cause(), payload())? }
assert match result { Err(_) => true; Ok(_) => false }
assert order == 21
"#);
    execute_both(&ordered);
}

#[test]
fn checked_snapshot_boolean_aliases_preserve_nullable_fields() {
    let program = fixture(r#"
type Row = {left: Int?, right: Int?}
pure sum(row: Row) -> Int {
    let present = row.left != null and row.right != null
    if ! present { return 0 }
    row.left + row.right
}
assert sum({left: 2, right: 3}) == 5
assert sum({left: null, right: 3}) == 0
pure compare(state: Str, value: Str?, target: Str) -> Int {
    if state != "observed" or value == null { 0 } else if value == target { 1 } else { 2 }
}
assert compare("observed", "kept", "kept") == 1
assert compare("observed", null, "kept") == 0
"#);
    execute_both(&program);
}

#[test]
fn checked_boolean_aliases_authenticate_binding_and_snapshot_identity() {
    let program = fixture(r#"
type Row = {value: Int?}
var shared: Row = {value: 7}
proc clear_shared() [] -> Unit { shared = {value: null} }
pure sum(row: Row, spare: Bool) -> Int {
    let present = row.value != null
    if ! present { return 0 }
    row.value
}
proc read_copy(row: Row) [] -> Int {
    let observed = shared.value
    let present = row.value != null
    clear_shared()
    if ! present { return 0 }
    row.value
}
assert sum({value: 7}, true) == 7
assert sum({value: null}, true) == 0
assert read_copy({value: 7}) == 7
"#);
    execute_both(&program);
    let mut original = program.clone();
    original.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&original).unwrap();
    execute_both(&original);
    let owner = function(&program, "sum");
    let decoder = function_view_decoder(&program.store, owner);
    let reference = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
        let schema::ExprView::IfExpr { mut branches, .. } = decoder.expression_view(instruction).ok()? else { return None };
        let (condition, _) = branches.next()?.ok()?;
        let schema::ExprView::Param(slot) = decoder.expression_view(condition.raw() as usize).ok()? else { return None };
        (slot >= program.store.functions[owner].params.len as usize).then_some(condition.raw() as usize)
    }).unwrap();
    let mut wrong_alias = original.clone();
    wrong_alias.store.extra[program.store.data[reference].range().start as usize] = 1;
    assert!(matches!(function_view_decoder(&wrong_alias.store, owner).expression_view(reference).unwrap(), schema::ExprView::Param(1)));
    let error = FullVerifier::verify(&wrong_alias).unwrap_err();
    assert!(error.message.contains("checked value input disagrees") || error.message.contains("non-null branch proof"), "{error:?}");

    let owner = function(&program, "read_copy");
    let setter = function(&program, "clear_shared");
    let captures = program.store.functions[owner].captures;
    let setter_captures = program.store.functions[setter].captures;
    let capture = program.store.captures.iter().skip(captures.start as usize).take(captures.len as usize)
        .find(|capture| program.store.string(capture.name).unwrap() == "shared").copied().unwrap();
    let setter_capture = program.store.captures.iter().skip(setter_captures.start as usize).take(setter_captures.len as usize)
        .find(|capture| program.store.string(capture.name).unwrap() == "shared").unwrap();
    assert_eq!(capture.binding_identity, setter_capture.binding_identity);
    assert_ne!(capture.slot_and_flags & (1 << 31), 0);
    assert_ne!(setter_capture.slot_and_flags & (1 << 31), 0);
    let captured_slot = capture.slot_and_flags & !(1 << 31);
    let decoder = function_view_decoder(&program.store, owner);
    let reads = program.store.function_instruction_range(owner).unwrap().filter(|instruction|
        matches!(decoder.expression_view(*instruction), Ok(schema::ExprView::Param(0)))).collect::<Vec<_>>();
    assert!(reads.len() >= 2);
    let mut shared_alias = original;
    for read in reads {
        shared_alias.store.extra[program.store.data[read].range().start as usize] = captured_slot;
        assert!(matches!(function_view_decoder(&shared_alias.store, owner).expression_view(read).unwrap(), schema::ExprView::Param(slot) if slot == captured_slot as usize));
    }
    let error = FullVerifier::verify(&shared_alias).unwrap_err();
    assert!(error.message.contains("checked value input disagrees") || error.message.contains("non-null branch proof"), "{error:?}");
}

#[test]
fn checked_nullable_conditional_storage_preserves_actual_branch_types() {
    let program = fixture(r#"
type Table = {size: Int, growths: List[Int]}
pure choose(table: Table) -> List[Int]? {
    let events: List[Int]? = if table.size > 0 { table.growths } else { null }
    events
}
assert choose({size: 1, growths: [2, 3]}) == [2, 3]
assert choose({size: 0, growths: [2, 3]}) == null
pure trim(value: Str?) -> Str? { value?.trim() }
assert trim(" kept ") == "kept"
assert trim(null) == null
pure environment(target: Str) -> Record {
    match target {
        "a" => {shared: "value", first: "one"}
        "b" => {shared: "value", second: "two"}
        else => {shared: "fallback"}
    }
}
assert environment("a").shared == "value"
assert environment("b").shared == "value"
"#);
    execute_both(&program);
}

#[test]
fn checked_narrowed_return_values_preserve_result_lifting() {
    let program = fixture(r#"
pure take(value: Int?) -> Result[Int] {
    if value == null { return Ok(0) }
    value
}
assert take(7)? == 7
assert take(null)? == 0
pure mutable_take(flag: Bool) -> Result[Int] {
    var found = null
    if flag { found = 7 }
    guard found != null else { return Ok(0) }
    found
}
assert mutable_take(true)? == 7
assert mutable_take(false)? == 0
"#);
    execute_both(&program);
}

#[test]
fn checked_narrowed_result_tails_preserve_unsigned_domains() {
    let mut program = fixture(r#"
pure keep(valid: UInt?, raw: Int?) -> Result[UInt] {
    if valid == null { return Ok(0) }
    if raw == null { return Ok(0) }
    valid
}
pure widen(value: UInt?) -> Result[Int] {
    if value == null { return Ok(0) }
    value
}
assert keep(7, -1)? == 7
assert widen(7)? == 7
"#);
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    let owner = function(&program, "keep");
    let decoder = function_view_decoder(&program.store, owner);
    let source = decoder.instruction_range.clone().find_map(|instruction| {
        let schema::StmtView::Return { value } = decoder.statement_view(instruction).ok()? else { return None };
        matches!(decoder.expression_view(value.raw() as usize).ok()?, schema::ExprView::Param(0)).then_some(value.raw() as usize)
    }).unwrap();
    let mut malformed = program.clone();
    malformed.store.extra[program.store.data[source].range().start as usize] = 1;
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(source).unwrap(), schema::ExprView::Param(1)));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("contract") || error.message.contains("checked value"), "{error:?}");
}

#[test]
fn checked_scalar_initializers_authenticate_every_writer() {
    let program = fixture(r#"
pure count(base: Int, other: Str) -> Int {
    var result = base
    result = base
    result
}
pure flag(base: Bool, other: Str) -> Bool {
    var result = base
    result = base
    result
}
assert count(7, "bad") == 7
assert flag(true, "bad")
"#);
    execute_both(&program);
    let mut original = program.clone();
    original.store.checked_expr_types.retain(|fact| fact.is_contract());
    for name in ["count", "flag"] {
        let owner = function(&program, name);
        let facts = original.store.functions[owner].slot_type_facts;
        original.store.slot_type_facts[facts.start as usize..(facts.start + facts.len) as usize].fill(IR_NONE);
    }
    FullVerifier::verify(&original).unwrap();
    execute_both(&original);
    for name in ["count", "flag"] {
        let owner = function(&program, name);
        let decoder = function_view_decoder(&program.store, owner);
        let value = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::AssignInt { value, .. }) if name == "count" => Some(value.raw() as usize),
            Ok(schema::StmtView::AssignBool { value, .. }) if name == "flag" => Some(value.raw() as usize),
            Ok(schema::StmtView::Assign { value, .. }) => Some(value.raw() as usize),
            _ => None,
        }).unwrap();
        assert!(matches!(program.store.tags[value], FullTag::IntSlot | FullTag::BoolSlot | FullTag::ExprParam));
        let mut malformed = original.clone();
        malformed.store.extra[program.store.data[value].range().start as usize] = 1;
        let error = FullVerifier::verify(&malformed).unwrap_err();
        assert!(error.message.contains("contract") || error.message.contains("type") || error.message.contains("scalar"), "{error:?}");
    }
}

#[test]
fn checked_mutable_slices_preserve_initializer_contracts() {
    let program = fixture(r#"
pure clip(data: Bytes, split: Bool) -> Result[Bytes] {
    let records = if split { if data.is_empty() { [] } else { [data] } } else { [data] }
    for raw in records {
        var line = raw
        var end = line.len()
        end = end - 1
        line = line[..end]
        return Ok(line)
    }
    Ok(b"")
}
assert clip(b"ab", false)? == b"a"
assert clip(b"ab", true)? == b"a"
assert clip(b"", true)? == b""
"#);
    execute_both(&program);
}

#[test]
fn checked_collectors_prove_record_writers() {
    let program = fixture(r#"
type Row = {value: Int}
pure gather(items: List[Int]) -> List[Row] {
    let result: List[Row] = collect {
        for item in items { yield {value: item} }
    }
    result
}
pure gather_or_stop(items: List[Int]) -> Result[List[Row], Str] {
    let result: List[Row] = collect {
        for item in items {
            if item < 0 { return Err("stop") }
            yield {value: item}
        }
    }
    Ok(result)
}
assert gather([1, 2]) == [{value: 1}, {value: 2}]
assert gather([]) == []
assert gather_or_stop([1, 2])? == [{value: 1}, {value: 2}]
assert gather_or_stop([1, -1, 2]) == Err("stop")
"#);
    execute_both(&program);
    let owner = function(&program, "gather_or_stop");
    let payload = program.store.function_instruction_range(owner).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprStr).unwrap();
    let integer = program.store.function_instruction_range(owner).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprInt).unwrap();
    let mut malformed = program.clone();
    malformed.store.tags[payload] = FullTag::ExprInt;
    malformed.store.data[payload] = program.store.data[integer];
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("contract"), "{error:?}");
}


#[test]
fn checked_collectors_preserve_empty_outputs() {
    let program = fixture(r#"
type Row = {value: Int}
pure empty() -> List[Row] {
    let result: List[Row] = collect {}
    result
}
assert empty() == []
"#);
    execute_both(&program);
    let owner = function(&program, "empty");
    let decoder = function_view_decoder(&program.store, owner);
    let (gate, checked) = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
        Ok(schema::ExprView::CheckedValue { check: ValueCheckView::Ground(check), value, .. }) if program.store.tags[value.raw() as usize] == FullTag::ExprValueBlock => Some((instruction, check.ty.id)),
        _ => None,
    }).unwrap();
    let Type::List(item) = program.store.semantic.to_type(checked).unwrap() else { panic!("list gate") };
    let mut malformed = program.clone();
    malformed.store.checked_expr_types.retain(|fact| fact.is_contract());
    let nonempty = SemanticPoolBuilder::default().intern_type(&mut malformed.store.semantic, &Type::non_empty(*item)).unwrap();
    let word = program.store.data[gate].range().start as usize + 2;
    assert_eq!(program.store.extra[word], checked.raw());
    malformed.store.extra[word] = nonempty.raw();
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(gate).unwrap(), schema::ExprView::CheckedValue { check: ValueCheckView::Ground(check), .. } if check.ty.id == nonempty));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("checked value input has no producer contract"), "{error:?}");
}

#[test]
fn checked_collectors_preserve_heterogeneous_record_shapes() {
    let program = fixture(r#"
pure varied() -> List[Record] {
    let result: List[Record] = collect {
        yield {number: 1}
        yield {text: "two"}
    }
    result
}
assert varied().len() == 2
"#);
    execute_both(&program);
}

#[test]
fn checked_collectors_preserve_unsigned_runtime_boundary() {
    let source = r#"
pure unsigned(value: Int) -> List[UInt] {
    let result: List[UInt] = collect { yield value }
    result
}
"#;
    execute_both(&fixture(&format!("{source}\nassert unsigned(7)[0] == 7\n")));
    let failed = Arc::new(fixture(&format!("{source}\nlet failed = unsigned(-1)\n")));
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*failed.sources).clone());
        evaluator.indexed_program = Some(Arc::clone(&failed));
        evaluator.force_frames = force_frames;
        let error = (0..failed.driver_step_count().unwrap()).find_map(|step| evaluator.eval_indexed_driver_step(step, Span::new(failed.store.source_id, 0, 0)).unwrap().err()).unwrap();
        assert_eq!(error.kind, "type-error");
        assert!(error.message.contains("UInt constraint"), "{error:?}");
    }
}

#[test]
fn checked_collectors_authenticate_writes_and_capture_roots() {
    let program = fixture(r#"
type Row = {value: Int, flag: Bool}
var shared: List[Bool] = [false]
pure gather(items: List[Int], flag: Bool) -> List[Row] {
    if shared.len() != 1 { return [] }
    let result: List[Row] = collect {
        for item in items { yield {value: item, flag: flag} }
    }
    result
}
assert gather([7], true)[0].value == 7
"#);
    execute_both(&program);
    let owner = function(&program, "gather");
    let decoder = function_view_decoder(&program.store, owner);
    let range = program.store.function_instruction_range(owner).unwrap();
    let input = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
        Ok(schema::ExprView::CheckedValue { value, .. }) if program.store.tags[value.raw() as usize] == FullTag::ExprValueBlock => Some(value.raw() as usize),
        _ => None,
    }).unwrap();
    let schema::ExprView::ValueBlock { mut body, .. } = decoder.expression_view(input).unwrap() else { unreachable!() };
    assert_eq!(body.len(), 3);
    let declaration = body.next().unwrap().unwrap().raw() as usize;
    let schema::StmtView::Let { slot: accumulator, .. } = decoder.statement_view(declaration).unwrap() else { panic!("fresh accumulator") };
    body.next().unwrap().unwrap();
    let schema::StmtView::Value { value: result } = decoder.statement_view(body.next().unwrap().unwrap().raw() as usize).unwrap() else { panic!("accumulator result") };
    let result = result.raw() as usize;
    assert!(matches!(decoder.expression_view(result).unwrap(), schema::ExprView::Param(slot) if slot == accumulator));
    let capture = program.store.captures[program.store.functions[owner].captures.bounds(program.store.captures.len()).unwrap()].iter()
        .find(|capture| program.store.string(capture.name).unwrap() == "shared").unwrap();
    assert_ne!(capture.slot_and_flags & (1 << 31), 0);
    let capture_slot = (capture.slot_and_flags & !(1 << 31)) as usize;
    assert_ne!(capture_slot, accumulator);
    assert_ne!(capture.binding_identity, IR_NONE);
    let mut malformed = program.clone();
    malformed.store.extra[program.store.data[result].range().start as usize] = capture_slot as u32;
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(result).unwrap(), schema::ExprView::Param(slot) if slot == capture_slot));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("checked value input disagrees with its contract"), "{error:?}");
    let writer = range.clone().find(|&instruction| matches!(decoder.statement_view(instruction), Ok(schema::StmtView::Assign { slot, op: AssignOp::Add, .. }) if slot == accumulator)).unwrap();
    let mut malformed = program.clone();
    let set = malformed.store.assign_ops.len() as u32;
    malformed.store.assign_ops.push(AssignOp::Set);
    malformed.store.extra[program.store.data[writer].range().start as usize + 1] = set;
    assert!(matches!(function_view_decoder(&malformed.store, owner).statement_view(writer).unwrap(), schema::StmtView::Assign { op: AssignOp::Set, .. }));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("checked value input has no producer contract"), "{error:?}");
    let flag = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(1)))).unwrap();
    let yielded = range.clone().find_map(|instruction| {
        let Ok(schema::ExprView::Record(mut fields)) = decoder.expression_view(instruction) else { return None };
        let super::super::decode::RecordEntryView::Field(_, value) = fields.next()?.ok()? else { return None };
        Some(value.raw() as usize)
    }).unwrap();
    let mut malformed = program.clone();
    malformed.store.data[yielded] = program.store.data[flag];
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(yielded).unwrap(), schema::ExprView::Param(1)));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("contract"), "{error:?}");
}


#[test]
fn checked_collectors_keep_nested_writer_proofs_bounded() {
    for (writers, depth) in [(16, 4), (32, 8), (64, 16), (128, 32)] {
        let mut body = "if flag {\n".repeat(depth);
        for value in 0..writers { body.push_str(&format!("yield {{value: {value}}}\n")); }
        body.push_str(&"}\n".repeat(depth));
        let source = format!("type Row = {{value: Int}}\npure alternate() -> Bool {{ false }}\npure gather(flag: Bool) -> List[Row] {{ let result: List[Row] = collect {{\n{body}}}\nresult\n}}\nassert gather(true).len() == {writers}\nassert gather(false) == []\n");
        let program = fixture(&source);
        execute_both(&program);
        let owner = function(&program, "gather");
        let range = program.store.function_instruction_range(owner).unwrap();
        assert!(range.clone().filter(|&instruction| matches!(program.store.tags[instruction], FullTag::StmtIf | FullTag::StmtIfBool)).count() >= depth);
        let scalar = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprInt).unwrap();
        let boolean = program.store.function_instruction_range(function(&program, "alternate")).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
        let mut malformed = program.clone();
        malformed.store.tags[scalar] = FullTag::ExprBool;
        malformed.store.data[scalar] = program.store.data[boolean];
        let error = FullVerifier::verify(&malformed).unwrap_err();
        assert!(error.message.contains("contract"), "{error:?}");
    }
}

#[test]
fn checked_collectors_preserve_lexical_shadow_boundaries() {
    let program = fixture(r#"
type Row = {value: Int}
pure gather(value: Int) -> List[Row] {
    let result: List[Row] = collect {
        yield {value: value}
        if value > 0 {
            let value = value + 1
            yield {value: value}
        }
        for value in [value] { yield {value: value} }
        yield {value: value}
    }
    result
}
assert gather(3) == [{value: 3}, {value: 4}, {value: 3}, {value: 3}]
"#);
    execute_both(&program);
    let owner = function(&program, "gather");
    let decoder = function_view_decoder(&program.store, owner);
    let range = program.store.function_instruction_range(owner).unwrap();
    let local = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
        Ok(schema::StmtView::LetInt { slot, .. }) => Some(slot),
        _ => None,
    }).unwrap();
    let before = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(0)))).unwrap();
    let mut malformed = program.clone();
    malformed.store.extra[program.store.data[before].range().start as usize] = local as u32;
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(before).unwrap(), schema::ExprView::Param(slot) if slot == local));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("visible lexical declaration"), "{error:?}");
    let (loop_slot, iter) = range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
        Ok(schema::StmtView::For { slot, iter, .. }) => Some((slot, iter.raw() as usize)),
        _ => None,
    }).unwrap();
    let schema::ExprView::List(mut items) = decoder.expression_view(iter).unwrap() else { panic!("literal loop input") };
    assert_eq!(items.len(), 1);
    let before_loop = items.next().unwrap().unwrap().raw() as usize;
    assert!(matches!(decoder.expression_view(before_loop).unwrap(), schema::ExprView::Param(0)));
    let mut malformed = program.clone();
    malformed.store.extra[program.store.data[before_loop].range().start as usize] = loop_slot as u32;
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(before_loop).unwrap(), schema::ExprView::Param(slot) if slot == loop_slot));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("visible lexical declaration") || error.message.contains("active lexical slot"), "{error:?}");
}

#[test]
fn checked_comprehensions_preserve_nonempty_mapping() {
    let program = fixture(r#"
pure copy_nonempty(items: NonEmpty[Int]) {
    [item for item in items]
}
pure filter_nonempty(items: NonEmpty[Int]) {
    [item for item in items if item > 0]
}
pure nested_nonempty(items: NonEmpty[Int]) {
    [item for item in items for item in {if item > 0 {[item]} else {[]}}]
}
pure through_splice(items: NonEmpty[Int]) -> Int {
    copy_nonempty([@items]).first()
}
let copied: NonEmpty[Int] = copy_nonempty([4, 5])
let filtered: List[Int] = filter_nonempty([-1])
let nested: List[Int] = nested_nonempty([-1])
assert through_splice([6]) == 6
assert copied.first() == 4
assert copied.last() == 5
assert filtered == []
assert nested == []
"#);
    execute_both(&program);
}

#[test]
fn checked_nonempty_literals_authenticate_members_and_cardinality() {
    let program = fixture(r#"
pure first(items: NonEmpty[Int]) -> Int { items.first() }
pure boolean() -> Bool { true }
pure concrete() -> Int {
    let spare: List[Int] = []
    first([7])
}
pure spliced(items: NonEmpty[Int], plain: List[Int]) -> Int {
    let _ = plain
    first([@items])
}
assert concrete() == 7
assert spliced([8], []) == 8
"#);
    execute_both(&program);
    let owner = function(&program, "concrete");
    let range = program.store.function_instruction_range(owner).unwrap();
    let decoder = function_view_decoder(&program.store, owner);
    let mut empty = None;
    let mut nonempty = None;
    let mut member = None;
    for instruction in range {
        if let Ok(schema::ExprView::List(mut items)) = decoder.expression_view(instruction) {
            if items.len() == 0 { empty = Some(instruction); }
            else {
                nonempty = Some(instruction);
                member = Some(items.next().unwrap().unwrap().raw() as usize);
            }
        }
    }
    let (empty, nonempty, member) = (empty.unwrap(), nonempty.unwrap(), member.unwrap());
    let mut malformed = program.clone();
    malformed.store.data.swap(empty, nonempty);
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(nonempty).unwrap(), schema::ExprView::List(items) if items.len() == 0));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("ground list producer has no item contract"), "{error:?}");
    let mut malformed = program.clone();
    malformed.store.tags[member] = FullTag::ExprBool;
    let boolean = program.store.function_instruction_range(function(&program, "boolean")).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
    malformed.store.data[member] = program.store.data[boolean];
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
    let owner = function(&program, "spliced");
    let decoder = function_view_decoder(&program.store, owner);
    let splice = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| {
        let Ok(schema::ExprView::ListBuild(mut entries)) = decoder.expression_view(instruction) else { return None };
        let (true, value, _) = entries.next().unwrap().unwrap() else { panic!("splice") };
        Some(value.raw() as usize)
    }).unwrap();
    assert!(matches!(decoder.expression_view(splice).unwrap(), schema::ExprView::Param(0)));
    let mut malformed = program.clone();
    let plain = program.store.function_instruction_range(owner).unwrap().find(|&instruction|
        matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(1)))).unwrap();
    malformed.store.data[splice] = program.store.data[plain];
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(splice).unwrap(), schema::ExprView::Param(1)));
    let error = FullVerifier::verify(&malformed).unwrap_err();
    assert!(error.message.contains("contract") || error.message.contains("non-empty producer"), "{error:?}");
}

#[test]
fn checked_comprehensions_authenticate_nonempty_mapping_source() {
    let program = fixture(r#"
pure mapped(items: NonEmpty[Int], plain: List[Int]) -> NonEmpty[Int] {
    let _ = plain
    [item for item in items]
}
assert mapped([9], []).first() == 9
"#);
    execute_both(&program);
    let owner = function(&program, "mapped");
    let decoder = function_view_decoder(&program.store, owner);
    let range = program.store.function_instruction_range(owner).unwrap();
    let comprehension = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprListComp).unwrap();
    let schema::ExprView::ListComp { mut qualifiers, .. } = decoder.expression_view(comprehension).unwrap() else { unreachable!() };
    let super::super::decode::CompQualifierView::For { iter, .. } = qualifiers.next().unwrap().unwrap() else { panic!("mapping source") };
    let iter = iter.raw() as usize;
    assert!(matches!(decoder.expression_view(iter).unwrap(), schema::ExprView::Param(0)));
    let plain = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(1)))).unwrap();
    let mut malformed = program.clone();
    malformed.store.data[iter] = program.store.data[plain];
    assert!(matches!(function_view_decoder(&malformed.store, owner).expression_view(iter).unwrap(), schema::ExprView::Param(1)));
    let error = FullVerifier::verify(&malformed).expect_err("mapping a possibly empty source cannot retain NonEmpty");
    assert!(error.message.contains("ground list comprehension does not preserve its validation"), "{error:?}");
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
            *signature.return_ty = Type::Result(Box::new(Type::Bool), Box::new(Type::ProcessError));
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
fn tempdir_cleanup_propagation_requires_error_unless_captured() {
    on_large_stack(|| {
        let source = r#"
let incorrect_payload: Str = "wrong"
proc declared_at(target: Path) [fs, error] -> Result[Int] {
    tempdir scratch at target { 1 }
}
proc inferred_fresh() {
    tempdir scratch { 1 }
}
proc inferred_at(target: Path) {
    tempdir scratch at target { 1 }
}
proc captured_at(target: Path) [fs] -> Result[Result[Int]] {
    try { tempdir scratch at target { 1 } }
}
proc retried_at(target: Path) [fs] -> Result[Int] {
    retry [] { tempdir scratch at target { 1 }? }
}
tempdir parent {
    assert declared_at(fp"{parent}/declared")? == 1
    assert inferred_fresh()? == 1
    assert inferred_at(fp"{parent}/inferred")? == 1
    let captured = captured_at(fp"{parent}/captured")?
    assert captured? == 1
    assert retried_at(fp"{parent}/retried")? == 1
}
"#;
        let mut program = fixture(source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        let retried = function(&program, "retried_at");
        let decoder = function_view_decoder(&program.store, retried);
        let propagated_tempdir = program.store.function_instruction_range(retried).unwrap().find_map(|instruction| {
            let schema::ExprView::Try { value, .. } = decoder.expression_view(instruction).ok()? else { return None };
            let value = value.raw() as usize;
            (program.store.tags[value] == FullTag::ExprValueBlock).then_some(value)
        }).unwrap();
        let source_contract = program.store.checked_expr_types.iter()
            .find(|fact| fact.instruction as usize == propagated_tempdir)
            .expect("the checked TempDir Result type remains a source contract");
        assert!(source_contract.is_contract());
        assert_eq!(program.store.semantic.to_type(source_contract.type_id).unwrap(),
            Type::Result(Box::new(Type::Int), Box::new(Type::Error)));
        FullVerifier::verify(&program).unwrap();
        for name in ["inferred_fresh", "inferred_at"] {
            let owner = function(&program, name);
            let FullCallableContract::Ground(signature) = program.store.functions[owner].contract else {
                panic!("inferred tempdir effects are closed")
            };
            let signature = program.store.semantic.to_signature_with_budget(
                signature,
                0,
                &mut crate::runtime::eval::indexed::semantic::TypeMaterializationBudget::default(),
            ).unwrap();
            let effects = signature.effects.concrete().unwrap().unwrap();
            assert!(effects.contains(&Effect::Fs), "{name}: {effects:?}");
            assert!(effects.contains(&Effect::Error), "{name}: {effects:?}");
        }
        execute_both(&program);

        for restricted in [
            "proc restricted() [fs] -> Result[Int] { tempdir scratch { 1 } }\n",
            "proc restricted(target: Path) [fs] -> Result[Int] { tempdir scratch at target { 1 } }\n",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), restricted);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(
                &parsed.arena,
                restricted,
                CheckOptions::for_execution(),
            );
            assert!(checked.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation)
            }), "{restricted}: {:?}", checked.diagnostics);
        }

        let pure = "pure restricted() { tempdir scratch { 1 } }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), pure);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(
            &parsed.arena,
            pure,
            CheckOptions::for_execution(),
        );
        assert!(checked.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckPureEffect)
        }), "{:?}", checked.diagnostics);

        let mut missing = program.clone();
        missing.store.checked_expr_types.retain(|fact| fact.instruction as usize != propagated_tempdir);
        FullVerifier::verify(&missing).unwrap();
        execute_both(&missing);

        let payload = program.store.function_instruction_range(retried).unwrap().find(|&instruction| {
            matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Int(1)))
        }).unwrap();
        let wrong_payload = program.store.tags.iter().position(|tag| *tag == FullTag::ExprStr).unwrap();
        let mut wrong_producer = missing.clone();
        wrong_producer.store.tags[payload] = FullTag::ExprStr;
        wrong_producer.store.data[payload] = program.store.data[wrong_payload];
        let error = FullVerifier::verify(&wrong_producer).unwrap_err();
        assert!(error.message.contains("producer") || error.message.contains("contract"), "{error:?}");

        let missing_effect = without_effect(&missing, function(&missing, "declared_at"), Effect::Error);
        assert!(FullVerifier::verify(&missing_effect).unwrap_err().message.contains("requires effect error"));

        let mut forged = program.clone();
        let wrong_result = SemanticPoolBuilder::default().intern_type(
            &mut forged.store.semantic,
            &Type::Result(Box::new(Type::Str), Box::new(Type::Error)),
        ).unwrap();
        forged.store.checked_expr_types.iter_mut()
            .find(|fact| fact.instruction as usize == propagated_tempdir).unwrap().type_id = wrong_result;
        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(error.message.contains("producer") || error.message.contains("contract"), "{error:?}");

        for wrong_result in [
            Type::Result(Box::new(Type::Any), Box::new(Type::Error)),
            Type::Result(Box::new(Type::Int), Box::new(Type::Any)),
            Type::Result(Box::new(Type::Any), Box::new(Type::Any)),
        ] {
            let mut forged = program.clone();
            let wrong_type = SemanticPoolBuilder::default()
                .intern_type(&mut forged.store.semantic, &wrong_result)
                .unwrap();
            forged.store.checked_expr_types.iter_mut()
                .find(|fact| fact.instruction as usize == propagated_tempdir).unwrap().type_id = wrong_type;
            assert!(FullVerifier::verify(&forged).is_err(), "accepted forged TempDir Result contract {wrong_result:?}");
        }
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
        signature.effects = crate::sema::types::CallableEffects::finite([]);
        program.store.functions[owner].contract = FullCallableContract::Ground(
            SemanticPoolBuilder::default().intern_signature(&mut program.store.semantic, &signature).unwrap());
        let error = FullVerifier::verify(&program).unwrap_err();
        assert!(error.message.contains("unknown effect contract"), "{error:?}");
    });
}


#[test]
fn driver_projections_derive_wait_results_from_actual_writers() {
    on_large_stack(|| {
        let source = r#"
let started = spawn run true
let child = started?
let statuses = {
    let observed_started = started
    wait [child]?
}
assert {
    let observed_child = child
    statuses[0].ok
}
"#;
        let mut program = fixture(source);
        FullVerifier::verify(&program).unwrap();
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let wait = program.store.tags.iter().position(|tag| *tag == FullTag::ExprWait).unwrap();
        let decoder = driver_projection_decoder(&program, wait);
        let schema::ExprView::Wait { target, .. } = decoder.expression_view(wait).unwrap() else { panic!("wait producer") };
        let schema::ExprView::List(mut values) = decoder.expression_view(target.raw() as usize).unwrap() else { panic!("wait target list") };
        let argument = values.next().unwrap().unwrap().raw() as usize;
        assert!(matches!(decoder.expression_view(argument).unwrap(), schema::ExprView::Param(_)));
        let mut wrong_handle = program.clone();
        wrong_handle.store.extra[program.store.data[argument].range().start as usize] = driver_projection_slot(&program, &decoder, "started");
        let error = FullVerifier::verify(&wrong_handle).unwrap_err();
        assert_eq!(error.message, "wait target is neither ProcessHandle nor List[ProcessHandle]", "{error:?}");
        let index = program.store.tags.iter().position(|tag| *tag == FullTag::ExprIndex).unwrap();
        let decoder = driver_projection_decoder(&program, index);
        let schema::ExprView::Index { base, .. } = decoder.expression_view(index).unwrap() else { panic!("status readback index") };
        let base = base.raw() as usize;
        assert!(matches!(decoder.expression_view(base).unwrap(), schema::ExprView::Param(_)));
        let mut wrong_readback = program.clone();
        wrong_readback.store.extra[program.store.data[base].range().start as usize] = driver_projection_slot(&program, &decoder, "child");
        let error = FullVerifier::verify(&wrong_readback).unwrap_err();
        assert_eq!(error.message, "callable index producer has no checked collection element type", "{error:?}");
    });
}

fn driver_projection_decoder(program: &FullProgram, instruction: usize) -> FullDecoder<'_> {
    (0..program.store.driver_steps.len()).map(|step| verify_schemes::decoder_for_driver_step(&program.store, step).unwrap())
        .find(|decoder| decoder.instruction_range.contains(&instruction)).unwrap()
}

fn driver_projection_slot(program: &FullProgram, decoder: &FullDecoder<'_>, name: &str) -> u32 {
    let driver = program.store.driver_steps[driver_owner_index(decoder.owner).unwrap()];
    let range = driver.slots.bounds(program.store.driver_slots.len()).unwrap();
    program.store.driver_slots[range].iter().find(|slot| program.store.string(slot.name).unwrap() == name).unwrap().slot
}

#[test]
fn driver_projections_authenticate_common_value_roots_and_all_writers() {
    on_large_stack(|| {
        let source = r#"
type DriverRow = {value: Int}
var state = {rows: [{value: 1}], enabled: true, kind: "stable", unused: "unused"}
state = {rows: [{value: 2}], enabled: true, kind: "stable", unused: "unused"}
proc change(value: Int, wrong: Bool) [] {
    state.rows[0].value = value
}
pure project(rows: List[DriverRow], wrong: List[Str]) -> List[DriverRow] { rows }
change(3, false)
let alias = state
let {rows, enabled, kind, unused} = alias
let projected = project(rows, ["wrong"])
guard let {values, active} = Ok({values: projected, active: enabled}) else { exit 3 }
assert values[0].value == 3
assert active
assert state.enabled
assert kind == state.kind
"#;
        let mut program = fixture(source);
        FullVerifier::verify(&program).unwrap();
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let definition = |name: &str| program.store.driver_slots.iter().position(|slot|
            slot.flags & DRIVER_SLOT_DEFINITION != 0 && program.store.string(slot.name).unwrap() == name).unwrap();
        let mut wrong_unused = program.clone();
        wrong_unused.store.driver_slots[definition("unused")].type_id = program.store.driver_slots[definition("enabled")].type_id;
        let error = FullVerifier::verify(&wrong_unused).unwrap_err();
        assert_eq!(error.message, "storage declaration disagrees with its read contract", "{error:?}");
        let alias_identity = program.store.binding_identities.iter().position(|identity| program.store.string(identity.name).unwrap() == "alias").unwrap() as u32;
        let alias_step = program.store.driver_steps.iter().find(|step| {
            let range = step.writers.bounds(program.store.driver_writers.len()).unwrap();
            program.store.driver_writers[range].iter().any(|writer| writer.binding_identity == alias_identity)
        }).unwrap();
        let range = alias_step.slots.bounds(program.store.driver_slots.len()).unwrap();
        let read = range.into_iter().find(|slot| {
            let row = program.store.driver_slots[*slot];
            row.flags & DRIVER_SLOT_READ != 0 && program.store.string(row.name).ok() == Some("state")
        }).unwrap();
        let mut cyclic = program.clone();
        cyclic.store.driver_slots[read].binding_identity = alias_identity;
        cyclic.store.driver_slots[read].name = program.store.binding_identities[alias_identity as usize].name;
        let error = FullVerifier::verify(&cyclic).unwrap_err();
        assert_eq!(error.message, "driver slot identity is not the currently visible binding", "{error:?}");
        for name in ["change", "project"] {
            let error = FullVerifier::verify(&process_parameter_mutation(&program, name, 0, 1)).unwrap_err();
            assert_eq!(error.message, "ground producer value disagrees with its checked contract", "{name}: {error:?}");
        }
        let mut fields = Vec::new();
        for step in 0..program.store.driver_steps.len() {
            let decoder = verify_schemes::decoder_for_driver_step(&program.store, step).unwrap();
            for instruction in decoder.instruction_range.clone() {
                if let Ok(schema::ExprView::Field { name, .. }) = decoder.expression_view(instruction) {
                    fields.push((name.to_string(), instruction));
                }
            }
        }
        let boolean = fields.iter().find(|(name, _)| name == "enabled").unwrap().1;
        let text = fields.iter().find(|(name, _)| name == "kind").unwrap().1;
        let mut wrong_field = program.clone();
        wrong_field.store.extra[program.store.data[boolean].range().start as usize + 1] = program.store.extra[program.store.data[text].range().start as usize + 1];
        let error = FullVerifier::verify(&wrong_field).unwrap_err();
        assert_eq!(error.message, "condition producer disagrees with its control role", "{error:?}");
    });
}

#[test]
fn driver_projections_contextual_argument_carriers_prove_actual_words() {
    on_large_stack(|| {
        let source = r#"
pure descriptor(word: Path, wrong: Bool, more: List[Str], erased: Any, erased_more: List[Any]) -> Command {
    let stored: List[Any] = ["true", word]
    process.command_argv("true", ["true", word, @more, @[]], env: {})
}
let plan = descriptor(p"/bin/true", false, ["argument"], p"/bin/true", ["argument"])
"#;
        let mut program = fixture(source);
        FullVerifier::verify(&program).unwrap();
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "descriptor");
        let decoder = function_view_decoder(&program.store, owner);
        let call = decoder.instruction_range.clone().find(|instruction| program.store.tags[*instruction] == FullTag::ExprProcessCommandArgv).unwrap();
        let schema::ExprView::ProcessCommandArgv(command) = decoder.expression_view(call).unwrap() else { panic!("command argv") };
        let argument = command.argv.raw() as usize;
        let stored = decoder.instruction_range.clone().find_map(|instruction| match decoder.statement_view(instruction) {
            Ok(schema::StmtView::Let { slot, annotation: Some(_), .. }) => Some(slot),
            _ => None,
        }).unwrap();
        for slot in [stored as u32, 1] {
            let mut noncarrier = program.clone();
            noncarrier.store.extra[program.store.data[argument].range().start as usize] = slot;
            let error = FullVerifier::verify(&noncarrier).unwrap_err();
            assert!(error.message.starts_with("command argv has no concrete word list contract"), "{error:?}");
        }
        for (from, to) in [(0, 1), (0, 3), (2, 4)] {
            let mut wrong_word = program.clone();
            for instruction in decoder.instruction_range.clone() {
                if matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(slot)) if slot == from) {
                    wrong_word.store.extra[program.store.data[instruction].range().start as usize] = to as u32;
                }
            }
            let error = FullVerifier::verify(&wrong_word).unwrap_err();
            assert_eq!(error.message, "command argv element is not a concrete process word", "{from}->{to}: {error:?}");
        }
    });
}

#[test]
fn driver_projections_authenticate_zero_output_sources_and_envelopes() {
    on_large_stack(|| {
        let source = r#"
var state = {value: 1}
let alias = state
let {} = alias
guard let _ = Ok(alias) else { exit 3 }
"#;
        let mut program = fixture(source);
        FullVerifier::verify(&program).unwrap();
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let step = program.store.driver_steps.iter().find(|step| step.tag == FullDriverTag::LetRecord).unwrap();
        let source = program.store.payload(step.data.range()).unwrap()[0] as usize;
        assert!(matches!(program.store.tags[source], FullTag::ExprParam));
        let mut wrong_source = program.clone();
        wrong_source.store.tags[source] = FullTag::ExprBool;
        wrong_source.store.extra[program.store.data[source].range().start as usize] = 1;
        let error = FullVerifier::verify(&wrong_source).unwrap_err();
        assert_eq!(error.message, "driver record source field path is not a record", "{error:?}");
        let guard = program.store.tags.iter().position(|tag| *tag == FullTag::StmtGuard).unwrap();
        let mut wrong_envelope = program.clone();
        let range = program.store.data[guard].range().bounds(program.store.extra.len()).unwrap();
        wrong_envelope.store.extra[range.end - 1] = 1;
        let error = FullVerifier::verify(&wrong_envelope).unwrap_err();
        assert_eq!(error.message, "driver guard writer has no success payload", "{error:?}");
    });
}

#[test]
fn driver_projections_contextual_lists_reject_guarded_multiarm_carriers() {
    on_large_stack(|| {
        let source = r#"
pure choose(flag: Bool) -> Command {
    let replacement = p"/bin/true"
    match ["true", "argument"] {
        values if flag => process.command_argv("true", values, env: {})
        values => process.command_argv("true", values, env: {})
    }
}
let plan = choose(false)
"#;
        let mut program = fixture(source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "choose");
        let decoder = function_view_decoder(&program.store, owner);
        let path = decoder.instruction_range.clone().find(|instruction| program.store.tags[*instruction] == FullTag::ExprPath).unwrap();
        let subject = decoder.instruction_range.clone().find_map(|instruction| {
            let schema::ExprView::MatchExpr { value, mut arms, .. } = decoder.expression_view(instruction).ok()? else { return None };
            let (_, guard, _) = arms.next()?.ok()?;
            (guard.is_some() && arms.next().is_some()).then_some(value.raw() as usize)
        }).unwrap();
        let schema::ExprView::List(mut values) = decoder.expression_view(subject).unwrap() else { panic!("match list subject") };
        let word = values.next().unwrap().unwrap().raw() as usize;
        let mut mixed = program.clone();
        mixed.store.tags[word] = program.store.tags[path];
        mixed.store.data[word] = program.store.data[path];
        let error = FullVerifier::verify(&mixed).unwrap_err();
        assert!(error.message.starts_with("command argv has no concrete word list contract"), "{error:?}");
    });
}

#[test]
fn driver_projections_wide_unused_destructuring_keeps_bounded_proof_work() {
    on_large_stack(|| {
        let fields = (0..256).map(|index| format!("field{index}: {index}")).collect::<Vec<_>>().join(", ");
        let names = (0..256).map(|index| format!("field{index}")).collect::<Vec<_>>().join(", ");
        let source = format!("var state = {{{fields}}}\nlet alias = state\nlet {{{names}}} = alias\nassert field255 == 255\n");
        let mut program = fixture(&source);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
    });
}

#[test]
fn physical_optional_record_lifts_validate_nested_fields() {
    on_large_stack(|| {
        let mut program = fixture(r#"
type Row = {value: Int, items: List[Int]}
pure foreign() -> Str { "bad" }
pure keep() -> Row? {
    var active: Row? = null
    active = {value: 7, items: []}
    active
}
assert keep() != null
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "keep");
        let scalar = program.store.function_instruction_range(owner).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprInt).unwrap();
        let foreign = program.store.tags.iter().position(|tag| *tag == FullTag::ExprStr).unwrap();
        let mut wrong_field = program.clone();
        wrong_field.store.tags[scalar] = FullTag::ExprStr;
        wrong_field.store.data[scalar] = program.store.data[foreign];
        let error = FullVerifier::verify(&wrong_field).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
    });
}

#[test]
fn physical_optional_pattern_reads_require_a_dominating_exit() {
    on_large_stack(|| {
        let mut program = fixture(r#"
enum Node { Item(Int?) }
pure take(value: Int) -> Result[Int] { Ok(value) }
pure initial() -> Int? { null }
pure keep(node: Node) -> Result[Int] {
    match node {
        Item(child) => {
            var scratch = initial()
            if child == null { return Ok(0) }
            scratch = null
            take(child)
        }
    }
}
assert keep(Item(7))? == 7
assert keep(Item(null))? == 0
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "keep");
        let range = program.store.function_instruction_range(owner).unwrap();
        let guard = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::BoolLiteralCompareSlot).unwrap();
        let condition = program.store.data[guard].range().start as usize;
        let guarded_slot = program.store.extra[condition + 1];
        let mut wrong_guard = program.clone();
        let op = program.store.extra[condition] as usize;
        wrong_guard.store.binary_ops[op] = BinaryOp::Ne;
        let error = FullVerifier::verify(&wrong_guard).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
        let writer = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::StmtAssign).unwrap();
        let mut stale_read = program.clone();
        stale_read.store.extra[program.store.data[writer].range().start as usize] = guarded_slot;
        let error = FullVerifier::verify(&stale_read).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
        let mut no_hints = program.clone();
        let facts = program.store.functions[owner].slot_type_facts;
        no_hints.store.slot_type_facts[facts.start as usize..(facts.start + facts.len) as usize].fill(IR_NONE);
        FullVerifier::verify(&no_hints).unwrap();
        execute_both(&no_hints);
        let source = r#"
enum Node { Item(Int?, Int) }
pure take(value: Int) -> Result[Int] { Ok(value) }
pure keep(node: Node) -> Result[Int] {
    match node {
        Item(child, checked) => {
            if GUARD { return Ok(0) }
            let _ = take(checked)
            let _ = take(checked)
            let _ = take(checked)
            take(checked)
        }
    }
}
assert keep(Item(7, 7))? == 7
assert keep(Item(null, 7))? == 0
"#.replace("GUARD", &format!("child == null{}", " == false".repeat(32)));
        let mut deep = fixture(&source);
        execute_both(&deep);
        let owner = function(&deep, "keep");
        let range = deep.store.function_instruction_range(owner).unwrap();
        assert_eq!(range.clone().filter(|&instruction| deep.store.tags[instruction] == FullTag::BoolNot).count(), 32);
        let decoder = function_view_decoder(&deep.store, owner);
        let comparison = range.clone().find(|&instruction| deep.store.tags[instruction] == FullTag::BoolLiteralCompareSlot).unwrap();
        let slot = deep.store.extra[deep.store.data[comparison].range().start as usize + 1];
        let target = function(&deep, "take");
        let arguments = range.filter_map(|instruction| {
            let schema::ExprView::DirectPureCall { function, mut args, .. } = decoder.expression_view(instruction).ok()? else { return None };
            if function.id.index() != target { return None; }
            let decode::CallArgView::Single(value) = args.next()?.unwrap() else { panic!("single argument") };
            Some(value.raw() as usize)
        }).collect::<Vec<_>>();
        assert_eq!(arguments.len(), 4);
        for argument in arguments {
            deep.store.extra[deep.store.data[argument].range().start as usize] = slot;
        }
        deep.store.checked_expr_types.retain(|fact| fact.is_contract());
        let facts = deep.store.functions[owner].slot_type_facts;
        deep.store.slot_type_facts[facts.start as usize..(facts.start + facts.len) as usize].fill(IR_NONE);
        FullVerifier::verify(&deep).unwrap();
        execute_both(&deep);
    });
}

#[test]
fn physical_optional_aggregate_shapes_do_not_establish_validation() {
    on_large_stack(|| {
        let mut program = fixture(r#"
pure empty() -> List[Int]? { [] }
assert (empty() ?? [1]).is_empty()
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "empty");
        let FullCallableContract::Ground(signature) = program.store.functions[owner].contract else { panic!("ground function") };
        let mut signature = program.store.semantic.to_signature_with_budget(signature, 0,
            &mut crate::runtime::eval::indexed::semantic::TypeMaterializationBudget::default()).unwrap();
        *signature.return_ty = Type::Optional(Box::new(Type::non_empty(Type::Int)));
        let signature = SemanticPoolBuilder::default().intern_signature(&mut program.store.semantic, &signature).unwrap();
        program.store.functions[owner].contract = FullCallableContract::Ground(signature);
        program.store.functions[owner].checked_return_type = program.store.semantic.signature_return_type(signature).unwrap().raw();
        let error = FullVerifier::verify(&program).unwrap_err();
        assert!(error.message.contains("ground list producer has no item contract"), "{error:?}");
    });
}

#[test]
fn physical_optional_pattern_active_branch_preserves_narrowing() {
    on_large_stack(|| {
        for body in [
            "if child != null { take(child) } else { return Ok(0) }",
            "if child != null { take(child) } else { Ok(0) }",
            "if child != null { return take(child) }; Ok(0)",
            "while child != null { return take(child) }; Ok(0)",
        ] {
            let source = r#"
enum Node { Item(Int?) }
pure take(value: Int) -> Result[Int] { Ok(value) }
pure keep(node: Node) -> Result[Int] {
    match node { Item(child) => { BODY } }
}
assert keep(Item(7))? == 7
assert keep(Item(null))? == 0
"#.replace("BODY", body);
            let mut program = fixture(&source);
            program.store.checked_expr_types.retain(|fact| fact.is_contract());
            FullVerifier::verify(&program).unwrap();
            execute_both(&program);
            let owner = function(&program, "keep");
            let guard = program.store.function_instruction_range(owner).unwrap().find(|&instruction| matches!(program.store.tags[instruction], FullTag::BoolLiteralCompareSlot | FullTag::ExprBinary)).unwrap();
            let op = program.store.extra[program.store.data[guard].range().start as usize] as usize;
            let mut wrong_arm = program.clone();
            wrong_arm.store.binary_ops[op] = BinaryOp::Eq;
            let error = FullVerifier::verify(&wrong_arm).unwrap_err();
            assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
            let facts = program.store.functions[owner].slot_type_facts;
            program.store.slot_type_facts[facts.start as usize..(facts.start + facts.len) as usize].fill(IR_NONE);
            FullVerifier::verify(&program).unwrap();
            execute_both(&program);
        }
    });
}

#[test]
fn physical_optional_pattern_null_guards_do_not_validate_unsigned() {
    on_large_stack(|| {
        for source in [r#"
enum Node { Item(Int?, UInt) }
pure take(value: UInt) -> Result[UInt] { Ok(value) }
pure keep(node: Node) -> Result[UInt] {
    match node {
        Item(child, checked) => {
            if child == null { return Ok(0) }
            take(checked)
        }
    }
}
assert keep(Item(-1, 7))? == 7
"#, r#"
type Signed = {value: Int}
type Unsigned = {value: UInt}
enum Node { Item(Signed?, Unsigned) }
pure take(value: Unsigned) -> Result[UInt] { Ok(value.value) }
pure keep(node: Node) -> Result[UInt] {
    match node {
        Item(child, checked) => {
            if child == null { return Ok(0) }
            take(checked)
        }
    }
}
assert keep(Item({value: -1}, {value: 7}))? == 7
"#] {
            let mut program = fixture(source);
            program.store.checked_expr_types.retain(|fact| fact.is_contract());
            FullVerifier::verify(&program).unwrap();
            execute_both(&program);
            let owner = function(&program, "keep");
            let decoder = function_view_decoder(&program.store, owner);
            let range = program.store.function_instruction_range(owner).unwrap();
            let guard = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::BoolLiteralCompareSlot).unwrap();
            let slot = program.store.extra[program.store.data[guard].range().start as usize + 1];
            let target = function(&program, "take");
            let argument = range.clone().find_map(|instruction| {
                let schema::ExprView::DirectPureCall { function, mut args, .. } = decoder.expression_view(instruction).ok()? else { return None };
                if function.id.index() != target { return None; }
                let decode::CallArgView::Single(value) = args.next()?.unwrap() else { panic!("single argument") };
                Some(value.raw() as usize)
            }).unwrap();
            assert!(matches!(decoder.expression_view(argument).unwrap(), schema::ExprView::Param(_)));
            let mut forged = program.clone();
            forged.store.extra[program.store.data[argument].range().start as usize] = slot;
            let error = FullVerifier::verify(&forged).unwrap_err();
            assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
        }
        let mut lawful = fixture(r#"
enum Node { Item(UInt?) }
pure take(value: Int) -> Result[Int] { Ok(value) }
pure keep(node: Node) -> Result[Int] {
    match node {
        Item(child) => {
            if child == null { return Ok(0) }
            take(child)
        }
    }
}
assert keep(Item(7))? == 7
assert keep(Item(null))? == 0
"#);
        lawful.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&lawful).unwrap();
        execute_both(&lawful);
    });
}

#[test]
fn producer_corpus_reduced_empty_list_argument_carrier() {
    on_large_stack(|| {
        let program = fixture(r#"
type Row = {items: List[Str]}
let row = Row(items: [])
assert row.items.is_empty()
"#);
        execute_both(&program);
    });
}


#[test]
fn producer_corpus_reduced_empty_map_argument_carrier() {
    on_large_stack(|| {
        let mut program = fixture(r#"
type Row = {before: Int, items: Map[Str, Int], after: Int}
var counter = 0
proc next() [] -> Int { counter += 1; counter }
let row = Row(after: next(), items: {}, before: next())
assert row.items.is_empty()
assert row.after == 1
assert row.before == 2
assert counter == 2
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
    });
}



#[test]
fn producer_context_immutable_branch_splice() {
    on_large_stack(|| {
        let mut program = fixture(r#"
pure scalar() -> Int { 7 }
pure words(flag: Bool) -> List[Str] {
    let prefix = if flag { [] } else { ["a"] }
    [@prefix, "b"]
}
assert words(true) == ["b"]
assert words(false) == ["a", "b"]
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "words");
        let item = program.store.function_instruction_range(owner).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprStr).unwrap();
        let scalar = program.store.tags.iter().position(|tag| *tag == FullTag::ExprInt).unwrap();
        let mut wrong_item = program.clone();
        wrong_item.store.tags[item] = FullTag::ExprInt;
        wrong_item.store.data[item] = program.store.data[scalar];
        let error = FullVerifier::verify(&wrong_item).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract") || error.message.contains("fresh list splice disagrees"), "{error:?}");
    });
}

#[test]
fn producer_context_carrier_contracts_require_unique_consumers() {
    on_large_stack(|| {
        for fields in ["left: List[Str], right: List[Str]", "left: List[Str], right: List[Int]"] {
            let source = format!("type Pair = {{{fields}}}\nlet pair = Pair(left: [], right: [])\nassert pair.left.is_empty()\nassert pair.right.is_empty()\n");
            let mut program = fixture(&source);
            program.store.checked_expr_types.retain(|fact| fact.is_contract());
            FullVerifier::verify(&program).unwrap();
            execute_both(&program);
            let mut fields = Vec::new();
            for step in 0..program.store.driver_steps.len() {
                let mut decoder = stage_view_decoder(&program.store);
                let row = program.store.driver_steps[step];
                decoder.owner = driver_owner(step).unwrap();
                decoder.slot_count = row.slot_count;
                decoder.instruction_range = program.store.driver_instruction_range(step).unwrap();
                for instruction in decoder.instruction_range.clone() {
                    if program.store.tags[instruction] != FullTag::ExprRecord { continue; }
                    let schema::ExprView::Record(mut entries) = decoder.expression_view(instruction).unwrap() else { unreachable!() };
                    let writers = decoder.checked_record_field_writers(&mut entries, |_| unreachable!()).unwrap();
                    for writer in writers.values() {
                        if let CheckedRecordFieldWriter::Value(value) = writer
                            && program.store.tags[*value] == FullTag::ExprParam { fields.push(*value); }
                    }
                }
            }
            assert_eq!(fields.len(), 2);
            let mut wrong_binder = program.clone();
            let first = program.store.data[fields[0]].range().start as usize;
            let second = program.store.data[fields[1]].range().start as usize;
            wrong_binder.store.extra[second] = program.store.extra[first];
            let error = FullVerifier::verify(&wrong_binder).unwrap_err();
            assert!(error.message.contains("contextual aggregate subject has no authenticated consumer contract") || error.message.contains("conflicting consumer contracts"), "{error:?}");
        }
    });
}

#[test]
fn producer_context_overwritten_record_entries_keep_their_obligations() {
    on_large_stack(|| {
        let mut program = fixture(r#"
type Row = {value: Int}
pure earlier() -> Int { 7 }
pure alternate() -> Bool { false }
pure keep() -> Result[Row] { Ok({value: earlier(), ...{value: 9}}) }
assert keep()?.value == 9
var counter = 0
proc next() [] -> Int { counter += 1; counter }
let observed = {value: next(), ...{value: next()}}
assert observed.value == 2
assert counter == 2
let failed = try { {value: "bad".parse_int()?, ...{value: next()}} }
assert match failed {
    Ok(_) => false
    Err(_) => true
}
assert counter == 2
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let earlier = function(&program, "earlier");
        let scalar = program.store.function_instruction_range(earlier).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprInt).unwrap();
        let alternate = function(&program, "alternate");
        let boolean = program.store.function_instruction_range(alternate).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
        let mut wrong_child = program.clone();
        wrong_child.store.tags[scalar] = FullTag::ExprBool;
        wrong_child.store.data[scalar] = program.store.data[boolean];
        let error = FullVerifier::verify(&wrong_child).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");

    });
}

#[test]
fn producer_context_checked_records_preserve_source_domains() {
    on_large_stack(|| {
        for body in ["Row(value: 7)", "{...{value: 7}}"] {
            let source = format!("type Row = {{value: Int}}\npure alternate() -> Bool {{false}}\npure keep() -> Result[Row] {{ let row: Row = {body}; Ok(row) }}\nassert keep()?.value == 7\n");
            let mut program = fixture(&source);
            program.store.checked_expr_types.retain(|fact| fact.is_contract());
            FullVerifier::verify(&program).unwrap();
            execute_both(&program);
            let keep = function(&program, "keep");
            let scalar = program.store.function_instruction_range(keep).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprInt).unwrap();
            let alternate = function(&program, "alternate");
            let boolean = program.store.function_instruction_range(alternate).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
            let mut wrong_field = program.clone();
            wrong_field.store.tags[scalar] = FullTag::ExprBool;
            wrong_field.store.data[scalar] = program.store.data[boolean];
            let error = FullVerifier::verify(&wrong_field).unwrap_err();
            assert!(error.message.contains("checked record field disagrees with its contract") || error.message.contains("checked record spread field disagrees with its contract") || error.message.contains("checked value input disagrees with its contract"), "{error:?}");
        }
        let program = fixture(r#"
type Row = {value: UInt, items: NonEmpty[Int]}
pure make(value: Int) -> Result[Row] { Ok(Row(value: value, items: [7])) }
assert make(7)?.value == 7
"#);
        execute_both(&program);
        let failed = Arc::new(fixture(r#"
type Row = {value: UInt, items: NonEmpty[Int]}
pure make(value: Int) -> Result[Row] { Ok(Row(value: value, items: [7])) }
let failed = make(-1)?
"#));
        for force_frames in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*failed.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&failed));
            evaluator.force_frames = force_frames;
            let error = (0..failed.driver_step_count().unwrap()).find_map(|step| evaluator.eval_indexed_driver_step(step, Span::new(failed.store.source_id, 0, 0)).unwrap().err()).unwrap();
            assert_eq!(error.kind, "type-error");
            assert!(error.message.contains("UInt constraint"), "{error:?}");
        }
        {
            let source = "type Row = {count: UInt}\npure alternate() -> Bool { false }\npure keep() -> Row? { let row: Row? = {count: 7}; row }\nassert keep() != null\n";
            let mut optional = fixture(source);
            optional.store.checked_expr_types.retain(|fact| fact.is_contract());
            FullVerifier::verify(&optional).unwrap();
            execute_both(&optional);
            let keep = function(&optional, "keep");
            let scalar = optional.store.function_instruction_range(keep).unwrap().find(|&instruction| matches!(optional.store.tags[instruction], FullTag::ExprInt | FullTag::ExprUInt)).unwrap();
            let alternate = function(&optional, "alternate");
            let boolean = optional.store.function_instruction_range(alternate).unwrap().find(|&instruction| optional.store.tags[instruction] == FullTag::ExprBool).unwrap();
            let mut wrong_field = optional.clone();
            wrong_field.store.tags[scalar] = FullTag::ExprBool;
            wrong_field.store.data[scalar] = optional.store.data[boolean];
            let error = FullVerifier::verify(&wrong_field).unwrap_err();
            assert!(error.message.contains("checked record field disagrees with its contract") || error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
        }
        let mut nested = fixture(r#"
type Inner = {items: NonEmpty[Int]}
type Row = {nested: Inner}
pure alternate_nested() -> Bool { false }
pure make(flag: Bool) -> Result[Row] {
    let row: Row = if flag { {nested: {items: [7]}} } else { {nested: {items: [8]}} }
    Ok(row)
}
assert make(true)?.nested.items[0] == 7
assert make(false)?.nested.items[0] == 8
"#);
        nested.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&nested).unwrap();
        execute_both(&nested);
        let make = function(&nested, "make");
        let scalars = nested.store.function_instruction_range(make).unwrap().filter(|&instruction| nested.store.tags[instruction] == FullTag::ExprInt).collect::<Vec<_>>();
        assert_eq!(scalars.len(), 2);
        let alternate = function(&nested, "alternate_nested");
        let boolean = nested.store.function_instruction_range(alternate).unwrap().find(|&instruction| nested.store.tags[instruction] == FullTag::ExprBool).unwrap();
        let mut wrong_nested = nested.clone();
        for scalar in scalars {
            wrong_nested.store.tags[scalar] = FullTag::ExprBool;
            wrong_nested.store.data[scalar] = nested.store.data[boolean];
        }
        let error = FullVerifier::verify(&wrong_nested).unwrap_err();
        assert!(error.message.contains("checked value input disagrees with its contract") || error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
    });
}

#[test]
fn checked_trivial_flow_census_keeps_all_returns_and_writers() {
    let program = fixture(r#"
pure choose(flag: Bool) -> Int {
    var result = 0
    if flag { result = 7; return result } else { result = 3; return result }
}
assert choose(true) == 7
assert choose(false) == 3
"#);
    execute_both(&program);
    let owner = function(&program, "choose");
    program.symbol_owner().with_current(|| {
        let mut decoder = function_view_decoder(&program.store, owner);
        let range = decoder.instruction_range.clone();
        assert!(range.clone().filter(|&instruction| program.store.tags[instruction] == FullTag::StmtReturn).count() >= 2);
        decoder.instruction_states = Some(RefCell::new(vec![0; range.len()]));
        decoder.instruction_proof = Some(RefCell::new(in_place::InstructionProof::new(range.clone())));
        decoder.block_states = Some(OwnedBlockStates::new(decoder.owner, &owned_block_counts(&program.store)));
        let mut index = verify_schemes::GroundProducerIndex::new(&program.store).unwrap();
        decoder.producer_demand = index.demand();
        let body = [program.store.functions[owner].body];
        let mut cursor = FullCursor::new(&body);
        Vec::<BuildStmtId>::verify(&decoder, &mut cursor).unwrap();
        cursor.finish().unwrap();
        decoder.finish_function().unwrap();
        index.record(&decoder).unwrap();
        let statements = range.clone().filter(|&instruction| program.store.tags[instruction].kind() == schema::InstructionKind::Stmt).count();
        assert_eq!(index.census_instruction_work(), (range.len() + statements, range.len()));
        assert!(index.census_matches_depth_first(&decoder).unwrap());
    });
}

#[test]
fn producer_context_census_bounds_mixed_instruction_patterns() {
    on_large_stack(|| {
        for locals in [64, 128, 256, 512] {
            let declarations = (0..locals).map(|index| format!("let local_{index} = {index}; ")).collect::<String>();
            let source = format!("enum Node {{ Item(Int) }}\npure alternate() -> Int {{ 0 }}\npure keep(node: Node) -> Result[Int] {{ {declarations} match node {{ Item(value) => Ok(value) }} }}\nassert keep(Item(7))? == 7\n");
            let program = fixture(&source);
            FullVerifier::verify(&program).unwrap();
            execute_both(&program);
            let keep = function(&program, "keep");
            program.symbol_owner().with_current(|| {
                let mut decoder = function_view_decoder(&program.store, keep);
                let range = decoder.instruction_range.clone();
                decoder.instruction_states = Some(RefCell::new(vec![0; range.len()]));
                decoder.instruction_proof = Some(RefCell::new(in_place::InstructionProof::new(range)));
                decoder.block_states = Some(OwnedBlockStates::new(decoder.owner, &owned_block_counts(&program.store)));
                let mut index = verify_schemes::GroundProducerIndex::new(&program.store).unwrap();
                decoder.producer_demand = index.demand();
                let body = [program.store.functions[keep].body];
                let mut cursor = FullCursor::new(&body);
                Vec::<BuildStmtId>::verify(&decoder, &mut cursor).unwrap();
                cursor.finish().unwrap();
                decoder.finish_function().unwrap();
                index.record(&decoder).unwrap();
                let (visits, instructions) = index.census_instruction_work();
                let statements = decoder.instruction_range.clone().filter(|&instruction| program.store.tags[instruction].kind() == schema::InstructionKind::Stmt).count();
                assert_eq!(visits, instructions + statements + 1);
                assert!(index.census_matches_depth_first(&decoder).unwrap());
            });
            let decoder = function_view_decoder(&program.store, keep);
            let matched = decoder.instruction_range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprMatch).unwrap();
            let schema::ExprView::MatchExpr { mut arms, .. } = decoder.expression_view(matched).unwrap() else { unreachable!() };
            let (pattern, _, _) = arms.next().unwrap().unwrap();
            let binds = decoder.pattern_capture_slots(pattern.raw() as usize).unwrap();
            assert_eq!(binds.len(), 1);
            assert!(matches!(decoder.checked_pattern_view(pattern.raw()).unwrap(), schema::PatternView::Tag { .. }));
            let payload = program.store.pattern_data[pattern.raw() as usize].range();
            let slot_word = (payload.start + payload.len) as usize - 1;
            assert_eq!(program.store.extra[slot_word], *binds.first().unwrap() as u32);
            let mut wrong_pattern = program.clone();
            wrong_pattern.store.extra[slot_word] = 0;
            let error = FullVerifier::verify(&wrong_pattern).unwrap_err();
            assert!(error.message.contains("slot") || error.message.contains("capture"), "{error:?}");
            let mut wrong_owner = program.clone();
            let alternate = function(&program, "alternate");
            wrong_owner.store.function_instruction_starts[keep] = program.store.function_instruction_starts[alternate];
            let error = FullVerifier::verify(&wrong_owner).unwrap_err();
            assert!(error.message.contains("instruction") || error.message.contains("owner"), "{error:?}");
        }
    });
}

#[test]
fn producer_context_native_arguments_keep_nested_producer_proofs() {
    on_large_stack(|| {
        let mut program = fixture(r#"
pure source() -> Bytes { b"." }
pure alternate() -> Str { "." }
pure keep() -> Result[Path] { Ok(source() as Path) }
assert keep()? == p"."
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let source = function(&program, "source");
        let bytes = program.store.function_instruction_range(source).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprBytes).unwrap();
        let alternate = function(&program, "alternate");
        let text = program.store.function_instruction_range(alternate).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprStr).unwrap();
        let mut wrong_leaf = program.clone();
        wrong_leaf.store.tags[bytes] = FullTag::ExprStr;
        wrong_leaf.store.data[bytes] = program.store.data[text];
        let error = FullVerifier::verify(&wrong_leaf).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
        let keep = function(&program, "keep");
        let decoder = function_view_decoder(&program.store, keep);
        assert!(decoder.instruction_range.clone().any(|instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::ModuleCall { op: RuntimeOp::PathParseBytes, .. }))));
    });
}

#[test]
fn producer_context_carrier_hints_do_not_replace_consumer_contracts() {
    on_large_stack(|| {
        let mut program = fixture(r#"
type Row = {items: List[Str]}
pure alternate() -> Bool { false }
pure keep() -> Result[Row] { Ok(Row(items: [])) }
assert keep()?.items.is_empty()
"#);
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let keep = function(&program, "keep");
        let empty = program.store.function_instruction_range(keep).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprList).unwrap();
        let owner = ExecutableOwner::Function(IrFunctionId::new(keep).unwrap());
        let mut wrong_hint = program.clone();
        let ty = SemanticPoolBuilder::default().intern_type(&mut wrong_hint.store.semantic, &Type::List(Box::new(Type::Int))).unwrap();
        wrong_hint.store.checked_expr_types.push(FullCheckedExprType { instruction: empty as u32, type_id: ty, purpose: FullCheckedExprPurpose::ProducerContext(owner) });
        wrong_hint.store.checked_expr_types.sort_by_key(|fact| fact.instruction);
        let error = FullVerifier::verify(&wrong_hint).unwrap_err();
        assert!(error.message.contains("checked record field disagrees with its contract") || error.message.contains("contextual aggregate hint disagrees"), "{error:?}");
        let mut coordinated = program.clone();
        let ty = SemanticPoolBuilder::default().intern_type(&mut coordinated.store.semantic, &Type::List(Box::new(Type::Str))).unwrap();
        coordinated.store.checked_expr_types.push(FullCheckedExprType { instruction: empty as u32, type_id: ty, purpose: FullCheckedExprPurpose::ProducerContext(owner) });
        coordinated.store.checked_expr_types.sort_by_key(|fact| fact.instruction);
        let alternate = function(&program, "alternate");
        let boolean = program.store.function_instruction_range(alternate).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
        coordinated.store.tags[empty] = FullTag::ExprBool;
        coordinated.store.data[empty] = program.store.data[boolean];
        let error = FullVerifier::verify(&coordinated).unwrap_err();
        assert_eq!(error.message, "full IR contains an instruction or block not owned by the function body");
        let mut typed_items = fixture(r#"
type Row = {items: List[Str]}
pure alternate() -> Bool { false }
pure keep() -> Result[Row] { Ok(Row(items: ["word"])) }
assert keep()?.items[0] == "word"
"#);
        typed_items.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&typed_items).unwrap();
        execute_both(&typed_items);
        let item_owner = function(&typed_items, "keep");
        let list = typed_items.store.function_instruction_range(item_owner).unwrap().find(|&instruction| typed_items.store.tags[instruction] == FullTag::ExprList).unwrap();
        let item = typed_items.store.function_instruction_range(item_owner).unwrap().find(|&instruction| typed_items.store.tags[instruction] == FullTag::ExprStr).unwrap();
        let alternate_items = function(&typed_items, "alternate");
        let boolean_item = typed_items.store.function_instruction_range(alternate_items).unwrap().find(|&instruction| typed_items.store.tags[instruction] == FullTag::ExprBool).unwrap();
        let ty = SemanticPoolBuilder::default().intern_type(&mut typed_items.store.semantic, &Type::List(Box::new(Type::Str))).unwrap();
        typed_items.store.checked_expr_types.push(FullCheckedExprType { instruction: list as u32, type_id: ty, purpose: FullCheckedExprPurpose::ProducerContext(ExecutableOwner::Function(IrFunctionId::new(item_owner).unwrap())) });
        typed_items.store.checked_expr_types.sort_by_key(|fact| fact.instruction);
        typed_items.store.tags[item] = FullTag::ExprBool;
        typed_items.store.data[item] = typed_items.store.data[boolean_item];
        let error = FullVerifier::verify(&typed_items).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract") || error.message.contains("checked record field disagrees with its contract"), "{error:?}");
        let mut wrong_owner = wrong_hint.clone();
        wrong_owner.store.checked_expr_types.iter_mut().find(|fact| fact.instruction == empty as u32).unwrap().purpose = FullCheckedExprPurpose::ProducerContext(ExecutableOwner::Function(IrFunctionId::new(alternate).unwrap()));
        assert!(FullVerifier::verify(&wrong_owner).is_err());
        let mut wrong_instruction = wrong_hint;
        wrong_instruction.store.checked_expr_types.iter_mut().find(|fact| fact.instruction == empty as u32).unwrap().instruction = boolean as u32;
        wrong_instruction.store.checked_expr_types.sort_by_key(|fact| fact.instruction);
        assert!(FullVerifier::verify(&wrong_instruction).is_err());
    });
}

#[test]
fn producer_context_native_constructor_preparation_stays_within_work_bound() {
    on_large_stack(|| {
        let program = fixture(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/xsh/generic-constructors.xsh")));
        FullVerifier::verify(&program).unwrap();
    });
}

#[test]
fn result_unit_binding_tails_keep_single_success_projection() {
    let program = fixture(r#"
error CopyFailure = Failed(message: Str)
proc attributes(fail: Bool) [] {
    if fail { return Err(CopyFailure.Failed("failed")) }
}
proc forwarded(fail: Bool) [] {
    let applied = attributes(fail)
    if let Err(failure) = applied { return Err(failure) }
    applied
}
assert forwarded(false) is Ok(_)
assert forwarded(true) is Err(_)
"#);
    execute_both(&program);
}

#[test]
fn checked_optional_loop_fields_preserve_runtime_predicates() {
    let mut program = fixture(r#"
type State = {selected: Str?, wrong: Bool?}
pure read(state: State, items: List[Int]) -> Str {
    let _ = state.wrong
    if state.selected != null {
        for item in items {
            let _ = item
            return state.selected
        }
    }
    "none"
}
assert read({selected: null, wrong: true}, [1]) == "none"
assert read({selected: "kept", wrong: true}, [1]) == "kept"
"#);
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    let owner = function(&program, "read");
    let range = program.store.function_instruction_range(owner).unwrap();
    let decoder = function_view_decoder(&program.store, owner);
    let _symbols = program.symbol_owner().enter();
    let (gate, input, checked) = range.clone().find_map(|instruction| {
        let Ok(schema::ExprView::CheckedValue { check: ValueCheckView::Ground(check), value, .. }) = decoder.expression_view(instruction) else { return None };
        if check.schema.is_none() && program.store.semantic.to_type(check.ty.id).unwrap() == Type::Str
            && matches!(decoder.expression_view(value.raw() as usize), Ok(schema::ExprView::Field { name: "selected", .. }))
        { Some((instruction, value.raw() as usize, check.ty.id)) } else { None }
    }).unwrap();
    let wrong = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Field { name: "wrong", .. }))).unwrap();
    let discard = range.clone().find(|&instruction| matches!(decoder.statement_view(instruction), Ok(schema::StmtView::LetRecord { source, target: super::super::decode::CompTargetView::Discard, .. }) if source.raw() as usize == wrong)).unwrap();
    let mut wrong_payload = program.clone();
    let gate_value = program.store.data[gate].range().start as usize;
    let discarded_value = program.store.data[discard].range().start as usize;
    assert_eq!(program.store.extra[gate_value], input as u32);
    assert_eq!(program.store.extra[discarded_value], wrong as u32);
    wrong_payload.store.extra.swap(gate_value, discarded_value);
    assert!(matches!(function_view_decoder(&wrong_payload.store, owner).expression_view(gate).unwrap(), schema::ExprView::CheckedValue { value, .. } if value.raw() as usize == wrong));
    let error = FullVerifier::verify(&wrong_payload).unwrap_err();
    assert!(error.message.contains("contract"), "{error:?}");
    let mut weakened = program.clone();
    let optional = SemanticPoolBuilder::default().intern_type(&mut weakened.store.semantic, &Type::Optional(Box::new(Type::Str))).unwrap();
    let check_type = gate_value + 2;
    assert_eq!(program.store.extra[check_type], checked.raw());
    weakened.store.extra[check_type] = optional.raw();
    weakened.store.checked_expr_types.retain(|fact| fact.instruction as usize != gate);
    assert!(matches!(function_view_decoder(&weakened.store, owner).expression_view(gate).unwrap(), schema::ExprView::CheckedValue { check: ValueCheckView::Ground(check), .. } if check.ty.id == optional));
    let error = FullVerifier::verify(&weakened).unwrap_err();
    assert!(error.message.contains("contract"), "{error:?}");
    let condition = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Binary { op: BinaryOp::Ne, left, right, .. })
        if matches!(decoder.expression_view(left.raw() as usize), Ok(schema::ExprView::Field { name: "selected", .. }))
            && matches!(decoder.expression_view(right.raw() as usize), Ok(schema::ExprView::Null)))).unwrap();
    let mut null_input = program.clone();
    let operation = program.store.extra[program.store.data[condition].range().start as usize] as usize;
    assert_eq!(program.store.binary_ops[operation], BinaryOp::Ne);
    null_input.store.binary_ops[operation] = BinaryOp::Eq;
    assert!(matches!(function_view_decoder(&null_input.store, owner).expression_view(condition).unwrap(), schema::ExprView::Binary { op: BinaryOp::Eq, .. }));
    let error = FullVerifier::verify(&null_input).unwrap_err();
    assert!(error.message.contains("contract"), "{error:?}");
    let null_input = Arc::new(null_input);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*null_input.sources).clone());
        evaluator.force_frames = force_frames;
        evaluator.indexed_program = Some(Arc::clone(&null_input));
        let error = (0..null_input.driver_step_count().unwrap()).find_map(|step| evaluator.eval_indexed_driver_step(step, Span::new(null_input.store.source_id, 0, 0)).unwrap().err()).unwrap();
        assert_eq!(error.kind, "type-error");
        assert!(error.message.contains("Str constraint"), "{error:?}");
    }
}

#[test]
fn record_list_fields_authenticate_fresh_optional_items() {
    let mut program = fixture(r#"
enum Role { Accent }
type View = {roles: List[Role?]}
pure make(stored: List[Null], wrong: Bool) -> View {
    let _ = stored
    let _ = wrong
    {roles: [null]}
}
let view = make([null], false)
assert view.roles.len() == 1
assert view.roles[0] == null
"#);
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    let owner = function(&program, "make");
    let range = program.store.function_instruction_range(owner).unwrap();
    let decoder = function_view_decoder(&program.store, owner);
    let (record, list) = range.clone().find_map(|instruction| {
        let Ok(schema::ExprView::Record(mut entries)) = decoder.expression_view(instruction) else { return None };
        match entries.next() {
            Some(Ok(super::super::decode::RecordEntryView::Field(_, value))) => Some((instruction, value.raw() as usize)),
            _ => None,
        }
    }).unwrap();
    let schema::ExprView::List(mut items) = decoder.expression_view(list).unwrap() else { unreachable!() };
    assert_eq!(items.len(), 1);
    let leaf = items.next().unwrap().unwrap().raw() as usize;
    assert!(matches!(decoder.expression_view(leaf).unwrap(), schema::ExprView::Null));
    let boolean = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(1)))).unwrap();
    let mut wrong_leaf = program.clone();
    wrong_leaf.store.tags[leaf] = FullTag::ExprParam;
    wrong_leaf.store.data[leaf] = program.store.data[boolean];
    assert!(matches!(function_view_decoder(&wrong_leaf.store, owner).expression_view(leaf).unwrap(), schema::ExprView::Param(1)));
    let error = FullVerifier::verify(&wrong_leaf).unwrap_err();
    assert!(error.message.contains("contract"), "{error:?}");
    let stored = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(0)))).unwrap();
    let discard = range.clone().find(|&instruction| matches!(decoder.statement_view(instruction), Ok(schema::StmtView::LetRecord { source, target: super::super::decode::CompTargetView::Discard, .. }) if source.raw() as usize == stored)).unwrap();
    let schema::ExprView::Record(entries) = decoder.expression_view(record).unwrap() else { unreachable!() };
    let field_value = program.store.blocks[entries.block_identity_for_test().index()].instructions.bounds(program.store.extra.len()).unwrap().start + 3;
    let discarded_value = program.store.data[discard].range().start as usize;
    assert_eq!(program.store.extra[field_value], list as u32);
    assert_eq!(program.store.extra[discarded_value], stored as u32);
    let mut wrong_storage = program.clone();
    wrong_storage.store.extra.swap(field_value, discarded_value);
    let changed = function_view_decoder(&wrong_storage.store, owner);
    let schema::ExprView::Record(mut entries) = changed.expression_view(record).unwrap() else { unreachable!() };
    assert!(matches!(entries.next(), Some(Ok(super::super::decode::RecordEntryView::Field(_, value))) if value.raw() as usize == stored));
    assert!(matches!(changed.expression_view(stored).unwrap(), schema::ExprView::Param(0)));
    let error = FullVerifier::verify(&wrong_storage).unwrap_err();
    assert!(error.message.contains("contract"), "{error:?}");
}

#[test]
fn empty_set_record_fields_authenticate_fresh_receivers() {
    let definitions = r#"
type Box = {values: Set[Str]}
pure count(value: Box) -> Int { value.values.len() }
pure make(stored: List[Any], wrong: Bool) -> Int {
    let _ = stored
    let _ = "is_empty"
    let spare = [wrong]
    let _ = spare
    count({values: set.empty()}) + count({values: [].to_set()})
}
"#;
    let mut program = fixture(&format!("{definitions}\nassert make([true], false) == 0"));
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    let owner = function(&program, "make");
    let range = program.store.function_instruction_range(owner).unwrap();
    let decoder = function_view_decoder(&program.store, owner);
    assert!(range.clone().any(|instruction| matches!(decoder.expression_view(instruction),
        Ok(schema::ExprView::ModuleCall { op: xsh_registry::RuntimeOp::SetEmpty, .. }))));
    let (method, receiver) = range.clone().find_map(|instruction| match decoder.expression_view(instruction) {
        Ok(schema::ExprView::Method { receiver, name: "to_set", .. }) => Some((instruction, receiver.raw() as usize)),
        _ => None,
    }).unwrap();
    assert!(matches!(decoder.expression_view(receiver).unwrap(), schema::ExprView::List(values) if values.len() == 0));
    assert!(range.clone().any(|instruction| match decoder.expression_view(instruction) {
        Ok(schema::ExprView::Record(mut entries)) => entries.any(|entry| matches!(entry, Ok(super::super::decode::RecordEntryView::Field(_, value)) if value.raw() as usize == method)),
        _ => false,
    }));
    let nonempty = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::List(values)) if values.len() == 1)).unwrap();
    let mut wrong_items = program.clone();
    wrong_items.store.data.swap(receiver, nonempty);
    assert!(matches!(function_view_decoder(&wrong_items.store, owner).expression_view(receiver).unwrap(), schema::ExprView::List(values) if values.len() == 1));
    let error = FullVerifier::verify(&wrong_items).unwrap_err();
    assert!(error.message.contains("contract"), "{error:?}");
    let stored = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(0)))).unwrap();
    let discard = range.clone().find(|&instruction| matches!(decoder.statement_view(instruction), Ok(schema::StmtView::LetRecord { source, target: super::super::decode::CompTargetView::Discard, .. }) if source.raw() as usize == stored)).unwrap();
    let mut wrong_storage = program.clone();
    let method_receiver = program.store.data[method].range().start as usize;
    let discarded_value = program.store.data[discard].range().start as usize;
    assert_eq!(program.store.extra[method_receiver], receiver as u32);
    assert_eq!(program.store.extra[discarded_value], stored as u32);
    wrong_storage.store.extra.swap(method_receiver, discarded_value);
    assert!(matches!(function_view_decoder(&wrong_storage.store, owner).expression_view(method).unwrap(), schema::ExprView::Method { receiver, .. } if receiver.raw() as usize == stored));
    let error = FullVerifier::verify(&wrong_storage).unwrap_err();
    assert!(error.message.contains("contract"), "{error:?}");
    let selector = (1..=program.store.strings.len()).find(|&index| program.store.string(index as u32).unwrap() == "is_empty").unwrap() as u32;
    let mut wrong_method = program.clone();
    let name = program.store.data[method].range().start as usize + 1;
    assert_eq!(program.store.string(program.store.extra[name]).unwrap(), "to_set");
    wrong_method.store.extra[name] = selector;
    assert!(matches!(function_view_decoder(&wrong_method.store, owner).expression_view(method).unwrap(), schema::ExprView::Method { name: "is_empty", .. }));
    let error = FullVerifier::verify(&wrong_method).unwrap_err();
    assert!(error.message.contains("contract"), "{error:?}");

    let mut definition_only = fixture(definitions);
    definition_only.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&definition_only).unwrap();
}

#[test]
fn conditional_record_results_authenticate_every_arm() {
    let mut program = fixture(r#"
type Identity = {pid: Int}
type Row = {selected: Identity?}
enum Key { Home, Other }
pure choose(state: Row, key: Key, empty: Bool, erased: Record, spare: Bool) -> Row {
    let _ = erased
    let _ = spare
    match key {
        Home => if empty { {selected: null} } else { {selected: {pid: 7}} }
        else => { let _ = spare; state }
    }
}
let cleared = choose({selected: {pid: 2}}, Home, true, {ignored: true}, false)
assert cleared.selected == null
let selected = choose({selected: {pid: 2}}, Home, false, {ignored: true}, false)
assert selected.selected == {pid: 7}
let unchanged = choose({selected: {pid: 2}}, Other, false, {ignored: true}, false)
assert unchanged.selected == {pid: 2}
"#);
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    let owner = function(&program, "choose");
    let range = program.store.function_instruction_range(owner).unwrap();
    let decoder = function_view_decoder(&program.store, owner);
    let scalar = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Int(7)))).unwrap();
    let boolean = program.store.tags.iter().position(|tag| *tag == FullTag::ExprBool).unwrap();
    let mut wrong_leaf = program.clone();
    wrong_leaf.store.tags[scalar] = FullTag::ExprBool;
    wrong_leaf.store.data[scalar] = program.store.data[boolean];
    let error = FullVerifier::verify(&wrong_leaf).unwrap_err();
    assert!(error.message.contains("contract"), "{error:?}");
    let _symbols = program.symbol_owner().enter();
    let record = range.clone().find(|&instruction| {
        let Ok(schema::ExprView::Record(mut entries)) = decoder.expression_view(instruction) else { return false };
        matches!(entries.next(), Some(Ok(super::super::decode::RecordEntryView::Field(name, value)))
            if name == Name::intern("selected") && program.store.tags[value.raw() as usize] == FullTag::ExprNull)
            && entries.next().is_none()
    }).unwrap();
    let mut missing_field = program.clone();
    let schema::ExprView::Record(entries) = decoder.expression_view(record).unwrap() else { unreachable!() };
    let field = program.store.blocks[entries.block_identity_for_test().index()].instructions.bounds(program.store.extra.len()).unwrap().start + 2;
    assert_eq!(program.store.extra[field], Name::intern("selected").symbol().raw());
    missing_field.store.extra[field] = Name::intern("pid").symbol().raw();
    let changed = function_view_decoder(&missing_field.store, owner);
    let schema::ExprView::Record(mut entries) = changed.expression_view(record).unwrap() else { unreachable!() };
    assert!(matches!(entries.next(), Some(Ok(super::super::decode::RecordEntryView::Field(name, _))) if name == Name::intern("pid")));
    let error = FullVerifier::verify(&missing_field).unwrap_err();
    assert!(error.message.contains("contract"), "{error:?}");
    let state = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(0)))).unwrap();
    for replacement in [3, 4] {
        let mut wrong_arm = program.clone();
        wrong_arm.store.extra[program.store.data[state].range().start as usize] = replacement;
        assert!(matches!(function_view_decoder(&wrong_arm.store, owner).expression_view(state).unwrap(), schema::ExprView::Param(slot) if slot == replacement as usize));
        let error = FullVerifier::verify(&wrong_arm).unwrap_err();
        assert!(error.message.contains("contract"), "{error:?}");
    }
}

#[test]
fn context_scope_return_preserves_result_envelope() {
    let program = fixture(r#"
pure boolean() -> Bool { true }
proc scoped() [env] -> Result[Unit] {
    env ({XSH_SCOPE_RETURN: "inner"}) { return }
}
assert scoped() is Ok(_)
"#);
    let owner = function(&program, "scoped");
    let mut range = program.store.function_instruction_range(owner).unwrap();
    let scope = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprContextScope).unwrap();
    let mut wrong_scope = program.clone();
    wrong_scope.store.extra[program.store.data[scope].range().start as usize] = 2;
    assert!(FullVerifier::verify(&wrong_scope).is_err());
    let unit = range.find(|&instruction| program.store.tags[instruction] == FullTag::ExprUnit).unwrap();
    let boolean = program.store.function_instruction_range(function(&program, "boolean")).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
    let mut wrong_body = program.clone();
    wrong_body.store.tags[unit] = FullTag::ExprBool;
    wrong_body.store.data[unit] = program.store.data[boolean];
    wrong_body.store.checked_expr_types.retain(|fact| fact.is_contract());
    assert!(FullVerifier::verify(&wrong_body).is_err());
    assert!(FullVerifier::verify(&without_effect(&program, owner, Effect::Env)).is_err());
    let program = Arc::new(program);
    for force_frames in [false, true] {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
        evaluator.indexed_program = Some(Arc::clone(&program));
        evaluator.force_frames = force_frames;
        let original = evaluator.env.snapshot_clone();
        for step in 0..program.driver_step_count().unwrap() {
            evaluator.eval_indexed_driver_step(step, Span::new(program.store.source_id, 0, 0)).unwrap().unwrap();
        }
        assert_eq!(evaluator.env.snapshot_clone(), original);
    }
}

#[test]
fn list_add_proves_contextual_operands() {
    let mut program = fixture(r#"
type Row = {value: Int}
pure boolean() -> Bool { true }
pure joined(first: Bool) -> List[Row] {
    (if first { [] } else { [{value: 7}] }) + []
}
pure copied(values: List[Row], spare: List[Bool]) -> List[Row] { values + [] }
var order = 0
proc left() [] -> List[Int] { order = order * 10 + 1; [1] }
proc right() [] -> List[Int] { order = order * 10 + 2; [2] }
proc in_order() [] -> List[Int] { left() + right() }
assert joined(true).len() == 0
assert joined(false)[0].value == 7
assert copied([{value: 8}], [true])[0].value == 8
assert in_order() == [1, 2]
assert order == 12
"#);
    program.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    let owner = function(&program, "joined");
    let mut range = program.store.function_instruction_range(owner).unwrap();
    let add = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprBinary).unwrap();
    let op = program.store.extra[program.store.data[add].range().start as usize] as usize;
    assert_eq!(program.store.binary_ops[op], BinaryOp::Add);
    let mut wrong_op = program.clone();
    wrong_op.store.binary_ops[op] = BinaryOp::Sub;
    assert!(FullVerifier::verify(&wrong_op).is_err());
    let member = range.find(|&instruction| program.store.tags[instruction] == FullTag::ExprInt).unwrap();
    let boolean = program.store.function_instruction_range(function(&program, "boolean")).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::ExprBool).unwrap();
    let mut wrong_member = program.clone();
    wrong_member.store.tags[member] = FullTag::ExprBool;
    wrong_member.store.data[member] = program.store.data[boolean];
    assert!(FullVerifier::verify(&wrong_member).is_err());
    let owner = function(&program, "copied");
    let decoder = function_view_decoder(&program.store, owner);
    let read = program.store.function_instruction_range(owner).unwrap()
        .find(|&instruction| matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Param(0)))).unwrap();
    let mut wrong_stored = program.clone();
    wrong_stored.store.extra[program.store.data[read].range().start as usize] = 1;
    assert!(matches!(function_view_decoder(&wrong_stored.store, owner).expression_view(read), Ok(schema::ExprView::Param(1))));
    assert!(FullVerifier::verify(&wrong_stored).is_err());
}

#[test]
fn optional_reads_in_later_conditions_keep_false_guard_evidence() {
    for expression in [false, true] {
        let body = "if value == null { 0 } else if value == -1 { 2 } else if let Err(failure) = deliver(value) { 3 } else { 1 }";
        let body = if expression { format!("let answer = {body}\nanswer") } else { body.to_owned() };
        let mut program = fixture(&format!(r#"
pure deliver(value: Int) -> Result[Unit] {{ Ok() }}
pure selected(value: Int?, spare: Int?) -> Int {{ {body} }}
assert selected(null, null) == 0
assert selected(7, null) == 1
assert selected(-1, null) == 2
"#));
        program.store.checked_expr_types.retain(|fact| fact.is_contract());
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let owner = function(&program, "selected");
        let decoder = function_view_decoder(&program.store, owner);
        let mut range = program.store.function_instruction_range(owner).unwrap();
        assert!(range.clone().any(|instruction| matches!(program.store.tags[instruction], FullTag::StmtPatternIf | FullTag::ExprPatternIf)));
        let guard = range.clone().find(|&instruction| matches!(decoder.expression_view(instruction),
            Ok(schema::ExprView::Binary { op: BinaryOp::Eq, right, .. }) if program.store.tags[right.raw() as usize] == FullTag::ExprNull)).unwrap();
        let mut wrong_guard = program.clone();
        let op = program.store.extra[program.store.data[guard].range().start as usize] as usize;
        wrong_guard.store.binary_ops[op] = BinaryOp::Ne;
        assert!(FullVerifier::verify(&wrong_guard).is_err());
        let input = range.find_map(|instruction| match decoder.expression_view(instruction) {
            Ok(schema::ExprView::CheckedValue { check: ValueCheckView::Ground(check), value, .. }) if check.ty.materialize().unwrap() == Type::Int => Some(value.raw() as usize),
            _ => None,
        }).unwrap();
        assert!(matches!(decoder.expression_view(input), Ok(schema::ExprView::Param(0))));
        let mut wrong_binding = program.clone();
        wrong_binding.store.extra[program.store.data[input].range().start as usize] = 1;
        assert!(FullVerifier::verify(&wrong_binding).is_err());
    }
}

#[test]
fn prepared_erased_records_preserve_record_kind() {
    let program = fixture(r#"
type Row = {name: Str}
const value = {name: "one"}
pure preserved() -> Row { value }
assert preserved().name == "one"
"#);
    FullVerifier::verify(&program).unwrap();
    execute_both(&program);
    let constant = program.store.prepared_constants.iter().find(|constant| matches!(constant.0, LoweredValue::Record(_))).unwrap();
    let decoder = function_view_decoder(&program.store, function(&program, "preserved"));
    let verify = |value: &PreparedConstantValue<TypeId>, expected: &Type| {
        let mut budget = verify_schemes::SchemeInstantiationBudget::for_prepared_constants(&program.store, 0);
        decoder.verify_prepared_constant_type(value, expected, &mut |amount| budget.charge(amount))
    };
    verify(constant, &Type::ErasedRecord).unwrap();
    let mut wrong_kind = constant.clone();
    wrong_kind.0 = LoweredValue::Bool(false);
    assert!(verify(&wrong_kind, &Type::ErasedRecord).is_err());
    let _symbols = program.symbol_owner().enter();
    let LoweredValue::Record(fields) = &constant.0 else { unreachable!() };
    let entries = fields.iter().map(|(name, value)| (Name::intern(name), value.clone())).collect::<Vec<_>>();
    let mut vector = constant.clone();
    vector.0 = LoweredValue::RecordVec(Arc::new(entries.clone()));
    verify(&vector, &Type::ErasedRecord).unwrap();
    let mut duplicate = entries;
    duplicate.push(duplicate[0].clone());
    vector.0 = LoweredValue::RecordVec(Arc::new(duplicate));
    assert!(verify(&vector, &Type::ErasedRecord).unwrap_err().message.contains("repeats a field"));
    let mut wrong_field = constant.clone();
    let LoweredValue::Record(fields) = &mut wrong_field.0 else { unreachable!() };
    Arc::make_mut(fields).insert(Arc::from("name"), LoweredValue::Bool(false));
    let typed = Type::Record(Arc::new(BTreeMap::from([(Name::intern("name"), Type::Str)])));
    assert!(verify(&wrong_field, &typed).is_err());
}

#[test]
fn immutable_optional_field_copies_preserve_guarded_method_receivers() {
    let program = fixture(r#"
type Battery = {power: Int?, current: Int?, voltage: Int?}
pure watts(sample: Battery, wrong: Bool) -> Float? {
    if sample.power != null {
        let microwatts = sample.power
        return null when microwatts < 0
        microwatts.float() / 1000000.0
    } else if sample.current != null and sample.voltage != null {
        let {current, voltage, ..} = sample
        return null when current == 0 or voltage <= 0
        (if current < 0 { -current } else { current }).float() * voltage.float() / 1000000000000.0
    } else { null }
}
assert watts(Battery(power: 1000000, current: null, voltage: null), false) == 1.0
assert watts(Battery(power: null, current: null, voltage: null), false) == null
assert watts(Battery(power: -1, current: null, voltage: null), false) == null
assert watts(Battery(power: null, current: 1000000, voltage: 1000000), false) == 1.0
"#);
    execute_both(&program);
    let owner = function(&program, "watts");
    let decoder = function_view_decoder(&program.store, owner);
    let _symbols = program.symbol_owner().enter();
    let guard = program.store.function_instruction_range(owner).unwrap().find(|&instruction| {
        matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Binary { op: BinaryOp::Ne, left, right, .. })
            if program.store.tags[right.raw() as usize] == FullTag::ExprNull
                && matches!(decoder.expression_view(left.raw() as usize), Ok(schema::ExprView::Field { name, .. }) if name == Name::intern("current")))
    }).unwrap();
    let mut wrong_guard = program.clone();
    wrong_guard.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&wrong_guard).unwrap();
    let op = wrong_guard.store.extra[wrong_guard.store.data[guard].range().start as usize] as usize;
    wrong_guard.store.binary_ops[op] = BinaryOp::Eq;
    assert!(FullVerifier::verify(&wrong_guard).is_err());
    let mut copied_slots = Vec::new();
    for instruction in program.store.function_instruction_range(owner).unwrap() {
        if let Ok(schema::StmtView::LetRecord { target, .. }) = decoder.statement_view(instruction) {
            decoder.comp_target_written_slots(target, &mut copied_slots).unwrap();
        }
    }
    let receiver = program.store.function_instruction_range(owner).unwrap().find_map(|instruction| match decoder.expression_view(instruction) {
        Ok(schema::ExprView::Method { name: "float", receiver, .. }) => match decoder.expression_view(receiver.raw() as usize) {
            Ok(schema::ExprView::Param(slot)) if copied_slots.contains(&slot) => Some(receiver.raw() as usize),
            _ => None,
        },
        _ => None,
    }).unwrap();
    let mut wrong_slot = program.clone();
    wrong_slot.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&wrong_slot).unwrap();
    wrong_slot.store.extra[wrong_slot.store.data[receiver].range().start as usize] = 1;
    assert!(matches!(function_view_decoder(&wrong_slot.store, owner).expression_view(receiver), Ok(schema::ExprView::Param(1))));
    assert!(FullVerifier::verify(&wrong_slot).is_err());
}

#[test]
fn guarded_record_copies_preserve_capture_order() {
    let program = fixture(r#"
type Sample = {value: Int?}
var shared: Sample = Sample(7)
proc clear() [] -> Int { shared = Sample(null); 0 }
proc copied() [] -> Float {
    if shared.value != null {
        let {value, ..} = shared
        let cleared = clear()
        value.float() + cleared.float()
    } else { 0.0 }
}
assert copied() == 7.0
assert shared.value == null
"#);
    execute_both(&program);
    let owner = function(&program, "copied");
    let setter = function(&program, "clear");
    let decoder = function_view_decoder(&program.store, owner);
    let declaration = program.store.function_instruction_range(owner).unwrap()
        .find(|&instruction| program.store.tags[instruction] == FullTag::StmtLetRecord).unwrap();
    let call = program.store.function_instruction_range(owner).unwrap().find(|&instruction|
        matches!(decoder.expression_view(instruction), Ok(schema::ExprView::Call { function, .. }) if function.id.index() == setter)).unwrap();
    let statement = program.store.function_instruction_range(owner).unwrap().find(|&instruction|
        matches!(decoder.statement_view(instruction), Ok(schema::StmtView::Expr { value, .. } | schema::StmtView::Let { value, .. }) if value.raw() as usize == call)).unwrap();
    let words = program.store.blocks.iter().find_map(|block| {
        let words = block.instructions.bounds(program.store.extra.len())?;
        let values = &program.store.extra[words.clone()];
        (values[1..].contains(&(declaration as u32)) && values[1..].contains(&(statement as u32))).then_some(words)
    }).unwrap();
    let first = words.start + 1 + program.store.extra[words.clone()][1..].iter().position(|&value| value == declaration as u32).unwrap();
    let second = words.start + 1 + program.store.extra[words][1..].iter().position(|&value| value == statement as u32).unwrap();
    assert!(first < second);
    let mut wrong_order = program.clone();
    wrong_order.store.checked_expr_types.retain(|fact| fact.is_contract());
    FullVerifier::verify(&wrong_order).unwrap();
    wrong_order.store.extra.swap(first, second);
    assert!(FullVerifier::verify(&wrong_order).is_err());
}

#[test]
fn guarded_record_copy_reads_keep_bounded_proof_work() {
    for count in [16, 32, 64, 128] {
        let values = std::iter::repeat_n("value.float()", count).collect::<Vec<_>>().join(", ");
        let program = fixture(&format!(r#"
type Sample = {{value: Int?}}
pure copied(sample: Sample) -> List[Float] {{
    if sample.value != null {{ let {{value, ..}} = sample; [{values}] }} else {{ [] }}
}}
let result = copied(Sample(7))
assert result.len() == {count}
assert result[0] == 7.0
assert result[{last}] == 7.0
"#, last = count - 1));
        execute_both(&program);
    }
}

#[test]
fn mutable_record_spreads_preserve_initial_contextual_fields() {
    let program = fixture(r#"
type State = {at: Int, items: List[Int]}
pure updated(source: State) -> State {
    var current = {...source, items: []}
    current = {...current, at: current.at + 1}
    current
}
let result = updated(State(0, [7]))
assert result.at == 1
assert result.items.len() == 0
"#);
    execute_both(&program);
}

#[test]
fn generic_proc_commands_retain_checked_operations_and_payloads() {
    on_large_stack(|| {
        let program = fixture(r#"
proc sample(payload) {
    print "checked"
    eprint "checked"
    env XSH_GENERIC_COMMAND=inside { print "scoped" }
    cd p"." { print "directory" }
    time.sleep 0ms ?
    run true
    return payload
}
assert sample(7)? == 7
assert sample("word")? == "word"
"#);
        let owner = function(&program, "sample");
        assert!(matches!(program.store.functions[owner].contract, FullCallableContract::Scheme(_)));
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let mut forged = program.clone();
        let plan = forged.store.prepared_module_call_plans.iter_mut().find(|plan| matches!(plan.as_ref(), crate::runtime::eval::ModuleCallPlan::CommandArguments(_))).unwrap();
        let crate::runtime::eval::ModuleCallPlan::CommandArguments(plan) = Arc::make_mut(plan) else { unreachable!() };
        plan.supplied_slots[0] = !plan.supplied_slots[0];
        assert!(FullVerifier::verify(&forged).is_err());
    });
}

#[test]
fn generic_proc_terminal_commands_keep_the_unit_result_envelope() {
    on_large_stack(|| {
        let program = fixture(r#"
proc report(payload) { print "terminal" }
let integer: Result[Unit] = report(7)
let text: Result[Unit] = report("word")
assert integer == Ok()
assert text == Ok()
"#);
        let owner = function(&program, "report");
        assert!(matches!(program.store.functions[owner].contract, FullCallableContract::Scheme(_)));
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
    });
}

#[test]
fn recursive_proc_completion_arms_keep_application_and_formal_result_envelopes() {
    on_large_stack(|| {
        let program = fixture(r#"
pure scalar(value: Int) -> Int { value }
pure outcome(value: Int) -> Result[Int] { Ok(value) }
proc left(action, value, count: Int) {
    if count == 0 { action(value) } else { right(action, value, count - 1) }
}

proc right(action, value, count: Int) { left(action, value, count) }
assert left(scalar, 7, 0) == Ok(7)
assert left(scalar, 7, 2) == Ok(7)
assert right(outcome, 8, 0) == Ok(8)
assert right(outcome, 8, 2) == Ok(8)
proc descend(value, count: Int) {
    if count == 0 { value } else { descend(value, count - 1) }
}
let stored_ok: Result[Int, Str] = Ok(9)
let stored_error: Result[Int, Str] = Err("stored")
assert descend(stored_ok, 0) == Ok(stored_ok)
assert descend(stored_ok, 2) == Ok(stored_ok)
assert descend(stored_error, 0) == Ok(stored_error)
assert descend(stored_error, 2) == Ok(stored_error)
"#);
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let mut missing = program.clone();
        assert!(!missing.store.scheme_application_returns.is_empty());
        missing.store.scheme_application_returns.clear();
        assert!(FullVerifier::verify(&missing).is_err());
    });
}


fn assert_recursive_proc_return_expression_runtime(completion: &'static str, formal_completion: &'static str) {
    on_large_stack(move || {
        let source = format!(r#"
pure scalar(value: Int) -> Int {{ value }}
pure text(value: Str) -> Str {{ value }}
pure outcome(value: Int) -> Result[Int] {{ Ok(value) }}
proc contextual(flag: Bool) [] -> Result[List[Int]] {{ return if flag {{ [] }} else {{ [7] }} }}
let empty: List[Int] = []
assert contextual(true) == Ok(empty)
assert contextual(false) == Ok([7])
proc left(action, value, count: Int) {{ return {completion} }}
proc right(action, value, count: Int) {{ left(action, value, count) }}
assert left(scalar, 7, 0) == Ok(7)
assert left(scalar, 7, 2) == Ok(7)
assert right(text, "kept", 0) == Ok("kept")
assert right(text, "kept", 2) == Ok("kept")
assert left(outcome, 8, 0) == Ok(8)
assert left(outcome, 8, 2) == Ok(8)
proc descend(value, count: Int) {{ return {formal_completion} }}
let stored_ok: Result[Int, Str] = Ok(9)
let stored_error: Result[Int, Str] = Err("stored")
assert descend(stored_ok, 0) == Ok(stored_ok)
assert descend(stored_ok, 2) == Ok(stored_ok)
assert descend(stored_error, 0) == Ok(stored_error)
assert descend(stored_error, 2) == Ok(stored_error)
"#);
        let program = fixture(&source);
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
        let mut missing = program.clone();
        assert!(!missing.store.scheme_application_returns.is_empty());
        missing.store.scheme_application_returns.clear();
        assert!(FullVerifier::verify(&missing).is_err());
    });
}

#[test]
fn recursive_proc_return_if_keeps_callback_envelopes_at_runtime() {
    assert_recursive_proc_return_expression_runtime(
        "if count == 0 { action(value) } else { right(action, value, count - 1) }",
        "if count == 0 { value } else { descend(value, count - 1) }",
    );
}

#[test]
fn recursive_proc_return_match_keeps_callback_envelopes_at_runtime() {
    assert_recursive_proc_return_expression_runtime(
        "match count { 0 => action(value); _ => right(action, value, count - 1) }",
        "match count { 0 => value; _ => descend(value, count - 1) }",
    );
}


#[test]
fn recursive_proc_completion_keeps_discarded_constructor_roles() {
    on_large_stack(|| {
        let program = fixture(r#"
proc descend(value, count: Int) [] {
    let _ = {items: map.empty(), value}
    if count == 0 { value } else { descend(value, count - 1) }
}
assert descend(7, 0) == Ok(7)
assert descend(7, 2) == Ok(7)
assert descend("kept", 0) == Ok("kept")
assert descend("kept", 2) == Ok("kept")
"#);
        FullVerifier::verify(&program).unwrap();
        execute_both(&program);
    });
}

#[test]
fn producer_context_record_loop_fields_keep_empty_list_contracts() {
    on_large_stack(|| {
        for rows in ["[{args: []}, {args: [\"-z\"]}]", "[{args: [\"-z\"]}, {args: []}]", "[{args: [], labels: [\"a\"]}, {args: [\"-z\"], labels: []}]"] {
            let source = format!("pure alternate() -> Bool {{ false }}\npure sizes() -> List[Int] {{ collect {{ for case in {rows} {{ yield case.args.len() }} }} }}\nassert sizes() == {}\n", if rows.starts_with("[{args: []") { "[0, 1]" } else { "[1, 0]" });
            let program = fixture(&source);
            FullVerifier::verify(&program).unwrap();
            execute_both(&program);
            let sizes = function(&program, "sizes");
            let value = program.store.function_instruction_range(sizes).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprStr).unwrap();
            let boolean = program.store.tags.iter().position(|tag| *tag == FullTag::ExprBool).unwrap();
            let mut forged = program.clone();
            forged.store.tags[value] = FullTag::ExprBool;
            forged.store.data[value] = program.store.data[boolean];
            assert!(FullVerifier::verify(&forged).is_err(), "incompatible actual row item accepted: {rows}");
        }
    });
}

#[test]
fn producer_context_typed_comprehension_match_subjects() {
    on_large_stack(|| {
        let source = r#"
pure foreign() -> Str { "outside" }
pure first_size(items: List[Str]) -> Int {
    match [item for item in items] {
        [head, ..tail] => head.count_chars()
        [] => 0
    }
}
assert first_size(["a", "word"]) == 1
assert first_size([]) == 0
"#;
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let subject = parsed.arena.arena.source_rows().expr_ids(SourceId::new(0))
            .find(|&expression| matches!(parsed.arena.arena.expr(expression).kind, crate::syntax::arena::ArenaExprKind::ListComp { .. })).unwrap();
        assert_eq!(checked.expr_types.get(&parsed.arena.arena.expr(subject).span), Some(&Type::List(Box::new(Type::Str))));
        let program = fixture(source);
        execute_both(&program);
        let owner = function(&program, "first_size");
        let decoder = function_view_decoder(&program.store, owner);
        let range = program.store.function_instruction_range(owner).unwrap();
        let comprehension = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprListComp).unwrap();
        let schema::ExprView::ListComp { value, mut qualifiers, .. } = decoder.expression_view(comprehension).unwrap() else { unreachable!() };
        let super::super::decode::CompQualifierView::For { target: super::super::decode::CompTargetView::Slot(slot), iter, .. } = qualifiers.next().unwrap().unwrap() else { panic!("checked comprehension binding") };
        let scalar = range.clone().find(|&instruction| program.store.tags[instruction] == FullTag::ExprInt).unwrap();
        let mut wrong_item = program.clone();
        wrong_item.store.tags[value.raw() as usize] = FullTag::ExprInt;
        wrong_item.store.data[value.raw() as usize] = program.store.data[scalar];
        FullVerifier::verify(&wrong_item).expect_err("actual comprehension element must satisfy its matched consumers");
        let payload = program.store.data[comprehension].range().bounds(program.store.extra.len()).unwrap();
        let block = IrBlockId::from_raw(program.store.extra[payload.start + 1]).unwrap();
        let entries = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
        assert_eq!(program.store.extra[entries.start + 3], slot as u32);
        assert_eq!(program.store.extra[entries.start + 4], iter.raw());
        let mut missing_binding = program.clone();
        missing_binding.store.extra[entries.start + 3] = slot as u32 + 1;
        FullVerifier::verify(&missing_binding).expect_err("comprehension value requires its actual iteration binding");
        let foreign = program.store.function_instruction_range(function(&program, "foreign")).unwrap()
            .find(|&instruction| program.store.tags[instruction] == FullTag::ExprStr).unwrap();
        let mut foreign_iterable = program.clone();
        foreign_iterable.store.extra[entries.start + 4] = foreign as u32;
        FullVerifier::verify(&foreign_iterable).expect_err("comprehension iterable must belong to its actual owner");
    });
}

#[test]
fn native_readonly_argv_accepts_typed_comprehension_arguments() {
    on_large_stack(|| {
        for argument in ["[]", "[\"word\", p\"path\"]"] {
            let program = fixture(&format!("test argv {{ |ctx| let output = test.run_script(ctx, \"\", args: {argument})?; assert output.status == 0 }}\n"));
            FullVerifier::verify(&program).unwrap();
        }
        for words in ["[\"word\"]", "[p\"word\"]"] {
            for argument in ["[word for word in words]", "arguments"] {
                let source = format!("test argv {{ |ctx| let words = {words}; let arguments = [word for word in words]; let output = test.run_script(ctx, \"\", args: {argument})?; assert output.status == 0 }}\n");
                let program = fixture(&source);
                FullVerifier::verify(&program).unwrap();
                let comprehension = program.store.tags.iter().rposition(|tag| *tag == FullTag::ExprListComp).unwrap();
                let decoder = program.store.functions.iter().enumerate().find_map(|(index, _)| {
                    let range = program.store.function_instruction_range(index).unwrap();
                    range.contains(&comprehension).then(|| function_view_decoder(&program.store, index))
                }).unwrap();
                let schema::ExprView::ListComp { value, .. } = decoder.expression_view(comprehension).unwrap() else { unreachable!() };
                let scalar = program.store.tags.iter().position(|tag| *tag == FullTag::ExprInt).unwrap();
                let mut wrong_item = program.clone();
                wrong_item.store.tags[value.raw() as usize] = FullTag::ExprInt;
                wrong_item.store.data[value.raw() as usize] = program.store.data[scalar];
                FullVerifier::verify(&wrong_item).expect_err("readonly argv rejects an actual integer element");
            }
        }
    });
}

#[test]
fn producer_context_fresh_comprehensions_preserve_contextual_items() {
    on_large_stack(|| {
        let program = fixture(r#"
type Label = {name: Str?}
pure clear(items: List[Label]) -> List[Label] { [{...item, name: null} for item in items] }
let items: List[Label] = [{name: "word"}]
assert clear(items)[0].name == null
"#);
        execute_both(&program);
        let clear = function(&program, "clear");
        let null = program.store.function_instruction_range(clear).unwrap()
            .find(|&instruction| program.store.tags[instruction] == FullTag::ExprNull).unwrap();
        let integer = program.store.tags.iter().position(|tag| *tag == FullTag::ExprInt).unwrap();
        let mut wrong_item = program.clone();
        wrong_item.store.tags[null] = FullTag::ExprInt;
        wrong_item.store.data[null] = program.store.data[integer];
        let error = FullVerifier::verify(&wrong_item).unwrap_err();
        assert!(error.message.contains("ground producer value disagrees with its checked contract"), "{error:?}");
    });
}
