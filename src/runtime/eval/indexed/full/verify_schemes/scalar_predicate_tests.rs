use super::super::{FullBuilder, FullProgram, FullVerifier};
use crate::sema::check::Checker;
use std::path::PathBuf;
use std::sync::Arc;

fn program(source: &str) -> FullProgram {
    program_with_roots(
        source,
        vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("core")],
    )
}

fn standalone_program(source: &str) -> FullProgram {
    program_with_roots(source, Vec::new())
}

fn program_with_roots(source: &str, module_roots: Vec<PathBuf>) -> FullProgram {
    let name = "system-report-terminal-quote.xsh";
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        name,
        crate::loader::entry_source_from_text(name, source.to_owned()),
        module_roots,
    );
    let source_id = crate::source::SourceMap::files(&sources)
        .first()
        .map(crate::source::SourceFile::id)
        .expect("entry source is present");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena_with_sources_and_options(
        &parsed.arena,
        &sources,
        source_id,
        crate::sema::check::CheckOptions {
            reveal_types: true,
            embedded_bodies: true,
            ..crate::sema::check::CheckOptions::default()
        },
    );
    let declarations = Checker::compact_declarations(&parsed.arena, checked);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    FullBuilder::build_compact(
        &parsed.arena,
        &declarations,
        source,
        Arc::new(sources),
        source_id,
    )
    .unwrap_or_else(|error| panic!("full program construction failed: {error:?}"))
}

fn function_index(program: &FullProgram, name: &str) -> usize {
    program
        .store
        .functions
        .iter()
        .position(|row| program.store.string(row.name).is_ok_and(|found| found == name))
        .expect("function exists")
}

#[test]
fn imported_system_report_json_get_calls_have_slot_contracts() {
    let source = "use lib.system_report\npure render(report: Record) -> Result[Str] { system_report.render_text(report, false, false) }\nlet _ = render({})\n";
    let program = program(source);

    FullVerifier::verify(&program).expect("imported json.get calls have authenticated slot types");
}

fn json_get_program() -> FullProgram {
    standalone_program(r#"
pure two_arg(value: Any) -> Bool {
    if let Ok(found) = json.get(value, ["field"]) { found == null } else { false }
}
pure three_arg(value: Any) -> Bool { json.get(value, ["field"], null) == null }
let _ = two_arg({})
let _ = three_arg({})
"#)
}

fn module_call(
    program: &FullProgram,
    function_name: &str,
    op: crate::runtime::eval::RuntimeOp,
) -> (usize, Vec<usize>) {
    let function = function_index(program, function_name);
    let decoder = super::super::function_view_decoder(&program.store, function);
    let instruction = program
        .store
        .function_instruction_range(function)
        .unwrap()
        .find(|&instruction| {
            matches!(
                decoder.expression_view(instruction),
                Ok(super::super::schema::ExprView::ModuleCall {
                    op: candidate,
                    ..
                }) if candidate == op
            )
        })
        .expect("function contains json.get");
    let super::super::schema::ExprView::ModuleCall { args, .. } =
        decoder.expression_view(instruction).unwrap()
    else {
        panic!("instruction is not a module call")
    };
    let args = args
        .map(|argument| {
            argument
                .unwrap()
                .map(|argument| argument.raw() as usize)
                .expect("json.get argument is present")
        })
        .collect();
    (instruction, args)
}

fn json_get_call(program: &FullProgram, function_name: &str) -> (usize, Vec<usize>) {
    module_call(program, function_name, crate::runtime::eval::RuntimeOp::JsonGet)
}

fn remove_json_get_argument(program: &mut FullProgram, instruction: usize, index: usize) {
    let call_range = program.store.data[instruction]
        .range()
        .bounds(program.store.extra.len())
        .unwrap();
    let call_payload = &program.store.extra[call_range];
    let block_id = super::super::IrBlockId::from_raw(*call_payload.last().unwrap()).unwrap();
    let block_index = block_id.index();
    let argument_range = program.store.blocks[block_index]
        .instructions
        .bounds(program.store.extra.len())
        .unwrap();
    let count = program.store.extra[argument_range.start] as usize;
    assert_eq!(count, 2);
    assert_eq!(index, 1);
    assert_eq!(argument_range.len(), 1 + count * 2);
    program.store.extra[argument_range.start] = 1;
    program.store.blocks[block_index].instructions.len -= 2;
}

#[test]
fn json_get_return_contracts_follow_argument_shape() {
    let program = json_get_program();

    FullVerifier::verify(&program).expect("two- and three-argument json.get calls have distinct results");
    assert_eq!(json_get_call(&program, "two_arg").1.len(), 2);
    assert_eq!(json_get_call(&program, "three_arg").1.len(), 3);
}

#[test]
fn json_get_rejects_an_actual_argument_with_the_wrong_type() {
    let program = json_get_program();
    FullVerifier::verify(&program).unwrap();
    let (_, args) = json_get_call(&program, "two_arg");
    let path = args[1];
    let boolean = program
        .store
        .tags
        .iter()
        .position(|tag| *tag == super::super::FullTag::ExprBool)
        .expect("fixture contains a boolean expression");
    let mut malformed = program.clone();
    malformed.store.tags[path] = super::super::FullTag::ExprBool;
    malformed.store.data[path] = program.store.data[boolean];

    FullVerifier::verify(&malformed).expect_err("the native path parameter still requires List[Any]");
}

#[test]
fn json_get_rejects_a_missing_required_path_argument() {
    let program = json_get_program();
    FullVerifier::verify(&program).unwrap();
    let (call, _) = json_get_call(&program, "two_arg");
    let mut malformed = program.clone();
    remove_json_get_argument(&mut malformed, call, 1);

    FullVerifier::verify(&malformed).expect_err("the two-argument signature requires its path");
}

#[test]
fn json_get_rejects_a_typed_list_variable_as_a_contextual_literal() {
    let source = r#"
pure lookup(value: Any, paths: List[Any], narrow_path: List[Str]) -> Bool {
    let _ = shlex.join(narrow_path)
    if let Ok(found) = json.get(value, paths) { found == null } else { false }
}
let _ = lookup({}, ["field"], ["narrow"])
"#;
    let program = standalone_program(source);
    FullVerifier::verify(&program).unwrap();

    let function = function_index(&program, "lookup");
    let decoder = super::super::function_view_decoder(&program.store, function);
    let (_, args) = json_get_call(&program, "lookup");
    let narrow_path = program
        .store
        .function_instruction_range(function)
        .unwrap()
        .find(|&instruction| {
            matches!(
                decoder.expression_view(instruction),
                Ok(super::super::schema::ExprView::Param(2))
            )
        })
        .expect("the function reads its narrower list parameter");
    let mut malformed = program.clone();
    malformed.store.tags[args[1]] = super::super::FullTag::ExprParam;
    malformed.store.data[args[1]] = program.store.data[narrow_path];
    malformed
        .store
        .checked_expr_types
        .retain(|fact| fact.instruction != args[1] as u32);

    let failure = FullVerifier::verify(&malformed)
        .expect_err("typed List[Str] parameters retain ordinary invariant matching");
    assert!(failure.message.contains("ground native operation arguments have no matching contract"), "{failure:?}");
}

#[test]
fn contextual_native_list_checks_each_member_against_its_element_type() {
    let program = standalone_program(
        "pure quote_words() -> Str { shlex.join([\"word\"]) }\nassert quote_words() == \"word\"\nlet _ = false\n",
    );
    FullVerifier::verify(&program).unwrap();

    let (_, args) = module_call(
        &program,
        "quote_words",
        crate::runtime::eval::RuntimeOp::ShlexJoin,
    );
    let decoder = super::super::function_view_decoder(
        &program.store,
        function_index(&program, "quote_words"),
    );
    let super::super::schema::ExprView::List(mut values) =
        decoder.expression_view(args[0]).unwrap()
    else {
        panic!("argument is not a direct list literal")
    };
    let word = values.next().unwrap().unwrap().raw() as usize;
    let boolean = program
        .store
        .tags
        .iter()
        .position(|tag| *tag == super::super::FullTag::ExprBool)
        .expect("fixture contains a boolean expression");
    let mut malformed = program.clone();
    malformed.store.tags[word] = super::super::FullTag::ExprBool;
    malformed.store.data[word] = program.store.data[boolean];
    malformed
        .store
        .checked_expr_types
        .retain(|fact| fact.instruction != word as u32);

    FullVerifier::verify(&malformed).expect_err("the List[Str] contract still checks each contextual literal member");
}

#[test]
fn pattern_condition_rejects_a_rebound_capture_slot() {
    let source = "pure compare(value: Str, other: Str) -> Bool { if other == \"fallback\" { match value { bound => bound == \"target\" } } else { false } }\nlet _ = compare(\"\", \"\")\n";
    let program = standalone_program(source);
    FullVerifier::verify(&program).expect("the pattern capture has a projected string contract");

    let function = function_index(&program, "compare");
    let decoder = super::super::function_view_decoder(&program.store, function);
    let range = program.store.function_instruction_range(function).unwrap();
    let match_instruction = range
        .clone()
        .find(|&instruction| program.store.tags[instruction] == super::super::FullTag::ExprMatch)
        .expect("comparison is inside a match expression");
    let super::super::schema::ExprView::MatchExpr { mut arms, .. } =
        decoder.expression_view(match_instruction).unwrap()
    else {
        panic!("match instruction has another expression view")
    };
    let (pattern, _, _) = arms.next().unwrap().unwrap();
    let super::super::schema::PatternView::Bind { slot } =
        decoder.checked_pattern_view(pattern.raw()).unwrap()
    else {
        panic!("match arm does not bind its subject directly")
    };
    let bound_slot = slot;
    let other_parameter = range
        .filter_map(|instruction| {
            (program.store.tags[instruction] == super::super::FullTag::ExprParam)
                .then(|| decoder.expression_view(instruction).ok())
                .flatten()
                .and_then(|view| match view {
                    super::super::schema::ExprView::Param(slot) if slot != bound_slot => Some(slot),
                    _ => None,
                })
        })
        .next()
        .expect("function has another parameter slot");
    let pattern_index = pattern.raw() as usize;
    let payload = program.store.pattern_data[pattern_index]
        .range()
        .bounds(program.store.extra.len())
        .unwrap();
    let mut malformed = program.clone();
    malformed.store.extra[payload.start] = u32::try_from(other_parameter).unwrap();

    assert!(FullVerifier::verify(&malformed).is_err());
}
