use crate::loader::parse_script;
use crate::runtime::eval::lowered_ops::lowered_value_matches;
use crate::runtime::eval::{
    EvalFlow, Evaluator, LOWERED_METHOD_NAMES, LoweredBoolExpr, LoweredBytesView, LoweredCallArg,
    LoweredExpr, LoweredFmtPart, LoweredFunctionBlocker, LoweredFunctionKey, LoweredFunctionKind,
    LoweredIntExpr, LoweredPipelineStage, LoweredProcessCommandBuilderEntry, LoweredRecordEntry,
    LoweredStmt, LoweredStrView, LoweredTopLevelKind, LoweredType, LoweredValue, Span, Value,
    apply_question, probe_compact_lower_constructed_bodies, probe_compact_lower_function_units,
    value_to_argv_bytes,
};
use crate::runtime::value::{PathValue, error_constructor, run_error_from_status};
use crate::sema::check::Checker;
use crate::source::{SourceId, SourceMap};
use crate::symbol::{Name, QualifiedName};
use crate::syntax::arena::ArenaProgramBuilder;
use crate::syntax::parser::Parser;
use crate::trace::{
    TraceJsonlRenderer, TraceKind, TracebackFrame, TracebackFrameKind, TracebackRenderer,
};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Run a test body on a worker thread with an 8 MiB stack. A few lowering /
/// eval tests recurse deeply enough to overflow the default 2 MiB test-thread
/// stack, even though the real `xsh` binary (8 MiB main stack) runs them fine.
fn run_with_big_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn ok_question_unwraps_value() {
    let span = Span::new(SourceId::new(0), 0, 1);
    let mut events = Vec::new();

    let flow = apply_question(Value::ok(Value::Int(7)), span, Vec::new(), &mut events);

    assert_eq!(flow, EvalFlow::Value(Value::Int(7)));
    assert!(events.is_empty());
}

#[test]
fn err_question_propagates_and_traces() {
    let mut sources = SourceMap::new();
    let id = sources.add_file("sample.xsh", "Err(Error(kind: \"x\"))?\n");
    let span = Span::new(id, 21, 22);
    let mut events = Vec::new();
    let frames = vec![TracebackFrame {
        kind: TracebackFrameKind::Proc,
        name: "main".to_string(),
        definition_span: None,
        call_span: Some(Span::new(id, 0, 3)),
    }];

    let flow = apply_question(
        Value::err(error_constructor("x", "failed")),
        span,
        frames,
        &mut events,
    );

    let EvalFlow::Propagate(propagation) = flow else {
        panic!("expected propagation");
    };
    assert_eq!(propagation.error.error_kind(), Some("x"));
    let traceback = TracebackRenderer::new().render(&propagation.traceback, &sources);
    assert!(traceback.contains("proc main"));
    let jsonl = TraceJsonlRenderer::new().render_events(&events, &sources);
    assert!(jsonl.contains("\"kind\":\"result.propagate\""));
    assert!(jsonl.contains("\"error_kind\":\"x\""));
}

#[test]
fn scanner_shaped_pures_enter_lowered_registry() {
    let source = "pure scan_line(line: Str) -> Int {
  let n = line.byte_len()
  var index = 0
  var score = 0
  var in_string = false
  var delim = -1

  while index < n {
let ch = line.byte_at(index)
let next = line.byte_at(index + 1)

if in_string {
  if ch == delim {
    in_string = false
  } else {
    score += ch % 7
  }
} else if ch == 47 and next == 47 {
  return score
} else if ch == 34 or ch == 39 {
  in_string = true
  delim = ch
} else if ch != 32 and ch != 9 {
  score += 1
}

index += 1
  }

  return score
}

pure scan_many(line: Str, limit: Int) -> Int {
  var total = 0
  var i = 0

  while i < limit {
total += scan_line(line)
i += 1
  }

  return total
}

scan_many(\"ab \\\"cd\\\" // ef\", 3)
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("scanner-lowered-registry.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    let lowered_names = evaluator.lowered_pures.keys().cloned().collect::<Vec<_>>();

    assert!(
        evaluator
            .lowered_pures
            .contains_key(&Name::intern("scan_line")),
        "lowered={lowered_names:?}"
    );
    assert!(
        evaluator
            .lowered_pures
            .contains_key(&Name::intern("scan_many")),
        "lowered={lowered_names:?}"
    );
}

#[test]
fn compact_function_units_record_dependency_and_scc_metadata_without_top_level_execution() {
    let source = "pure even(n: Int) -> Bool {
  if n == 0 {
    return true
  }
  return odd(n - 1)
}

pure odd(n: Int) -> Bool {
  if n == 0 {
    return false
  }
  return even(n - 1)
}
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-function-units-scc.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);

    let units = probe_compact_lower_function_units(&parsed.arena, &declarations, &bodies, source);
    let even = units
        .iter()
        .find(|unit| unit.key() == LoweredFunctionKey::Name(Name::intern("even")))
        .expect("even unit");
    let odd = units
        .iter()
        .find(|unit| unit.key() == LoweredFunctionKey::Name(Name::intern("odd")))
        .expect("odd unit");

    assert_eq!(even.kind(), LoweredFunctionKind::Pure);
    assert!(even.is_lowered(), "{:?}", even.blocker());
    assert!(odd.is_lowered(), "{:?}", odd.blocker());
    assert_eq!(even.scc_member_count(), 2);
    assert_eq!(odd.scc_group(), even.scc_group());
    assert_eq!(
        even.dependency_edges(),
        &[LoweredFunctionKey::Name(Name::intern("odd"))]
    );
    assert_eq!(
        odd.dependency_edges(),
        &[LoweredFunctionKey::Name(Name::intern("even"))]
    );
}

#[test]
fn compact_function_units_record_structured_blockers() {
    let source = "pure missing_return(n: Int) -> Int {
  let value = n + 1
}
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-function-unit-blocker.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);

    let units = probe_compact_lower_function_units(&parsed.arena, &declarations, &bodies, source);
    let unit = units
        .iter()
        .find(|unit| unit.key() == LoweredFunctionKey::Name(Name::intern("missing_return")))
        .expect("missing_return unit");

    assert_eq!(unit.kind(), LoweredFunctionKind::Pure);
    assert!(!unit.is_lowered());
    assert_eq!(unit.blocker(), Some(LoweredFunctionBlocker::NoReturn));
    assert_eq!(
        unit.blocker().map(|blocker| blocker.label()),
        Some("no_return")
    );
    assert_eq!(unit.param_count(), 1);
    assert_eq!(unit.dependency_edges(), &[]);
}

#[test]
fn compact_install_replaces_root_lowered_function_entries() {
    let source = "pure inc(n: Int) -> Int {
  return n + 1
}

let value = inc(2)
print $value
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-installed-functions.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let name = Name::intern("inc");

    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    evaluator
        .lowered_pures
        .get(&name)
        .expect("compact lowered entry");

    let output = evaluator.eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"3\n");
    assert!(output.traceback.is_none());
}

#[test]
fn compact_installed_function_lowers_local_empty_map_context() {
    let source = "pure count_one(key: Str) -> Map[Int] {
  var counts: Map[Int] = {}
  counts = counts.set(key, counts.get(key, 0) + 1)
  return counts
}

let counts = count_one(\"x\")
print ${counts.get(\"x\", 0)}
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-local-map.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let name = Name::intern("count_one");

    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    evaluator
        .lowered_pures
        .get(&name)
        .expect("compact lowered entry");

    let output = evaluator.eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"1\n");
    assert!(output.traceback.is_none());
}

#[test]
fn compact_installed_functions_bootstrap_forward_sibling_calls() {
    let source = "pure caller(n: Int) -> Int {
  return callee(n) + 1
}

pure callee(n: Int) -> Int {
  return n * 2
}

print ${caller(3)}
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-forward-sibling.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let caller = Name::intern("caller");
    let callee = Name::intern("callee");

    evaluator.install_compact_lowered_program(&parsed.arena, source_id);

    evaluator
        .lowered_pures
        .get(&caller)
        .expect("compact caller lowered entry");
    evaluator
        .lowered_pures
        .get(&callee)
        .expect("compact callee lowered entry");

    let output = evaluator.eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"7\n");
    assert!(output.traceback.is_none());
}

#[test]
fn compact_installed_functions_colower_mutual_recursion() {
    let source = "pure even(n: Int) -> Bool {
  if n == 0 {
    return true
  }
  return odd(n - 1)
}

pure odd(n: Int) -> Bool {
  if n == 0 {
    return false
  }
  return even(n - 1)
}

print ${even(6)} ${odd(7)}
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-mutual-recursion.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let even = Name::intern("even");
    let odd = Name::intern("odd");

    evaluator.install_compact_lowered_program(&parsed.arena, source_id);

    evaluator
        .lowered_pures
        .get(&even)
        .expect("compact even lowered entry");
    evaluator
        .lowered_pures
        .get(&odd)
        .expect("compact odd lowered entry");

    let output = evaluator.eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"true true\n");
    assert!(output.traceback.is_none());
}

#[test]
fn compact_install_executes_without_old_root_function_registries() {
    let source = "pure double(n: Int) -> Int {
  return n * 2
}

pure add_one(n: Int) -> Int {
  return double(n) + 1
}

let value = add_one(4)
print $value
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-no-old-root-functions.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.lowered_pures.clear();
    evaluator.lowered_procs.clear();
    evaluator.lowered_program = Default::default();

    evaluator.install_compact_lowered_program(&parsed.arena, source_id);

    assert!(
        evaluator
            .lowered_pures
            .contains_key(&Name::intern("double"))
    );
    assert!(
        evaluator
            .lowered_pures
            .contains_key(&Name::intern("add_one"))
    );
    assert!(
        evaluator
            .lowered_program
            .statements
            .iter()
            .any(Option::is_some),
        "expected compact top-level IR without old root function registries"
    );

    let output = evaluator.eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"9\n");
    assert!(output.traceback.is_none());
}

#[test]
fn compact_lowered_only_eval_runs_arena_without_compat_program() {
    let source = "pure double(n: Int) -> Int {
  return n * 2
}

pure add_one(n: Int) -> Int {
  return double(n) + 1
}

add_one(4)
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-only-eval.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
    assert!(constructed.constructed_functions >= 2);
    assert!(constructed.constructed_top_level_statements >= 1);

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("covered compact program should not require compatibility Program");

    assert_eq!(output.status, 9);
    assert!(output.stdout.is_empty());
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty());
}

#[test]
fn compact_lower_constructs_annotated_record_field_path_binding() {
    let source = "type Options = {root: Path, scale: Int}
let opts: Options = {root: p\"/tmp/xsh-compact-record\", scale: 1}
let root = opts.root
fs.mkdir(fp\"${root}/child\")?

let parsed: Options = cli.parse(args, {root: {form: \"--root PATH\", kind: \"Path\", required: true}, scale: {form: \"--scale N\", default: 1}})?
let parsed_root = parsed.root
fs.mkdir(fp\"${parsed_root}/child\")?
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-record-field.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);

    assert_eq!(
        constructed.constructed_top_level_statements, 6,
        "{:?}",
        constructed.top_level_blockers
    );
}

#[test]
fn compact_lowered_only_lowers_structured_error_constructors() {
    let source = "error AppError = Usage(message: Str)

proc fail() -> Result[Int] {
  return Err(AppError.Usage(message: \"bad input\"))
}

fail()?
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-structured-error.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
    assert_eq!(constructed.functions, 1);
    assert_eq!(constructed.constructed_functions, 1);
    assert_eq!(constructed.call_blocker_callees.get("AppError.Usage"), None);
    assert_eq!(constructed.call_blocker_callees.get("Err"), None);

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("structured error constructors should not require compatibility Program");

    assert!(output.stdout.is_empty());
    assert!(output.traceback.is_some());
    assert!(output.diagnostics.is_empty());
}

#[test]
fn compact_lowered_only_lowers_abort_intrinsic() {
    let source = "abort(7)\n";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-abort.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
    assert_eq!(constructed.constructed_top_level_statements, 1);
    assert_eq!(constructed.call_blocker_callees.get("abort"), None);

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("abort should stay on the compact lowered path");

    assert_eq!(output.status, 7);
    assert!(output.stdout.is_empty());
    assert!(output.diagnostics.is_empty());
}

#[test]
fn compact_lower_constructs_qualified_module_calls() {
    let main_source = "use helper

pure call_helper(n: Int) -> Int {
  return helper.double(n)
}

call_helper(4)
";
    let module_source = "export pure double(n: Int) -> Int {
  return n * 2
}
";
    // Assemble the multi-module arena the way the loader does.
    let mut builder = ArenaProgramBuilder::with_token_capacity(main_source.len() / 4 + 1);
    let root = Parser::parse_source_into_arena_builder(SourceId::new(0), main_source, &mut builder);
    let module =
        Parser::parse_source_into_arena_builder(SourceId::new(1), module_source, &mut builder);
    assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
    assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
    for stmt in builder.statement_ids(root.statements) {
        if let Some((use_id, _path, _span)) = builder.use_stmt_for_statement(stmt) {
            builder.set_use_resolved(use_id, Arc::from("helper"));
        }
    }
    builder.push_arena_module(
        "helper".to_string(),
        Name::intern("helper"),
        module.statements,
    );
    let arena = builder.finish_with_statements(root.statements);
    let declarations = Checker::check_compact_declarations(&arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    assert!(
        declarations
            .qualified_pures
            .contains_key(&QualifiedName::new(
                Name::intern("helper"),
                Name::intern("double")
            ))
    );
    let bodies = Checker::probe_compact_bodies(&arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed = probe_compact_lower_constructed_bodies(
        &arena,
        &declarations,
        &bodies,
        "use helper

pure call_helper(n: Int) -> Int {
  return helper.double(n)
}

call_helper(4)
",
    );

    assert_eq!(constructed.functions, 2);
    assert_eq!(constructed.constructed_functions, 2);
}

#[test]
fn compact_body_probe_rejects_reassigning_let() {
    let source = "let x = 1\nx = 2\n";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-assign-let.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);

    assert!(
        bodies
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == Some("check.assign-let")),
        "{:?}",
        bodies.diagnostics
    );
}

#[test]
fn compact_lowered_only_accepts_definition_only_programs_without_proc_main() {
    let source = "pure id(n: Int) -> Int {
  return n
}
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-definition-only.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("definition-only program without proc main should not require compatibility AST");

    assert_eq!(output.status, 0);
    assert!(output.stdout.is_empty());
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty());
}

#[test]
fn compact_lowered_only_preserves_standard_record_param_checks() {
    let source = r#"
pure entry_name(entry: FsEntry) -> Str {
  return entry.name
}

let raw: Record = {
  path: "not a path",
  name: "demo",
  kind: "file",
  ext: "",
  size: 1,
  mode: 0,
  uid: 0,
  gid: 0,
  modified: 0,
  accessed: 0
}
print ${entry_name(raw)}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-standard-record-param.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("structural annotations should stay on the compact lowered path");

    assert!(output.stdout.is_empty());
    assert!(output.traceback.is_some());
    assert!(
        output.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("expected FsEntry, found Record")),
        "{:?}",
        output.diagnostics
    );
}

#[test]
fn compact_lowered_only_preserves_result_top_level_slots() {
    let source = r#"
pure parse_count(raw: Str) -> Result[Int] {
  return Ok(raw.parse_int()?)
}

let parsed: Result[Int] = parse_count("41")
let parsed_builtin = "1".parse_int()
let value = parsed?
let extra = parsed_builtin?
print ${value + extra}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-result-slot.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("result-typed top-level bindings should stay compact-only");

    assert_eq!(output.stdout, b"42\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_print_accepts_value_splices() {
    let source = r#"
let label = "alpha"
let items = ["zero", "one"]
let record = {name: "demo"}
print @label @("beta") items[1] record.name
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-print-splice.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("print/eprint value splices should stay compact-only");

    assert_eq!(output.stdout, b"alpha beta one demo\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_handles_scoped_cd() {
    let tests_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let original = std::env::current_dir().expect("current dir");
    let source = format!(
        "let root = fp{:?}\ncd root {{\n  print ${{fs.cwd()}}\n}}\nprint ${{fs.cwd()}}\n",
        tests_dir.display().to_string()
    );
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-cd.xsh", &source);
    let parsed = Parser::parse_source_arena_only(source_id, &source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("scoped cd should stay compact-only");

    let stdout = std::str::from_utf8(&output.stdout).expect("stdout is utf-8");
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(
        lines,
        vec![
            tests_dir.display().to_string(),
            original.display().to_string(),
        ]
    );
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_handles_scoped_env() {
    run_with_big_stack(|| {
        let source = r#"
    env XSH_COMPACT_SCOPED_ENV=inside {
      print ${env.get("XSH_COMPACT_SCOPED_ENV") ?? "missing"}
    }
    print ${env.get("XSH_COMPACT_SCOPED_ENV") ?? "unset"}
    "#;
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("compact-env.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

        let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
        let output = evaluator
            .eval_compact_lowered_only(&parsed.arena, source_id)
            .expect("scoped env should stay compact-only");

        assert_eq!(output.stdout, b"inside\nunset\n");
        assert_eq!(output.status, 0);
        assert!(output.traceback.is_none());
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    });
}

#[test]
fn compact_lowered_only_handles_terminal_pipeline_stages() {
    let source = r#"
let values = [3, 1, 2, 1]
let unique = values |> unique-by .
print ${values |> any . == 2}
print ${values |> all . > 0}
print ${(values |> first())?} ${(values |> last())?} ${([3, 1, 2] |> min)?} ${([3, 1, 2] |> max)?}
print ${unique.len()} ${unique[0]} ${unique[1]} ${unique[2]}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-terminal-pipelines.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("terminal pipeline stages should stay compact-only");

    assert_eq!(output.stdout, b"true\ntrue\n3 1 1 3\n3 3 1 2\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_handles_linux_network_dry_run_calls() {
    run_with_big_stack(|| {
        let source = r#"
    proc linux_network_controls() [env, error] -> Result[Int] {
      env XSH_LINUX_DRY_RUN=1 {
        linux.link_down("eth0")?
        linux.flush_ipv4_addresses("eth0")?
        linux.add_default_ipv4_route("192.0.2.1", interface: "eth0")?
        linux.del_default_ipv4_route("192.0.2.1", "eth0")?
        let fd = linux.dhcp_socket("eth0")?
        linux.dhcp_send(fd, b"abc")?
        let reply = linux.dhcp_recv(fd, 1)?
        linux.dhcp_close(fd)?
        linux.dhcp_send_release("eth0", "192.0.2.10", "192.0.2.1")?
        return reply.len()
      }
      return 0
    }

    print ${linux_network_controls()?}
    "#;
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("compact-linux-network.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let constructed =
            probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
        assert_eq!(
            constructed.function_blockers, [0; 6],
            "{:?}",
            constructed.function_body_tail_call_callees
        );

        let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
        let output = evaluator
            .eval_compact_lowered_only(&parsed.arena, source_id)
            .expect("linux network dry-run calls should stay compact-only");

        assert_eq!(output.stdout, b"0\n");
        assert_eq!(output.status, 0);
        assert!(output.traceback.is_none());
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    });
}

#[test]
fn compact_lowered_only_handles_path_methods_and_registry_modules() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("xsh-compact-path-registry-{nanos}"));
    fs::create_dir_all(&root).expect("create temp root");
    fs::write(root.join("source.txt"), "alpha\nbeta\n").expect("write source");

    let mut source = format!("let root = fp{:?}\n", root.display().to_string());
    source.push_str(
        r#"
let source = fp"${root}/source.txt"
let method_link = fp"${root}/method-link.txt"
let module_link = fp"${root}/module-link.txt"
let result_path = fp"${root}/result.txt"
let truncate_path = fp"${root}/truncate.txt"
let ini_path = fp"${root}/config.ini"
let copied_path = fp"${root}/copied.txt"
let renamed_path = fp"${root}/renamed.txt"
let stamp_path = fp"${root}/stamp.txt"
let empty_dir = fp"${root}/empty"
let unlink_path = fp"${root}/unlink.txt"
let _method_linked = source.hardlink(method_link)?
let _method_chmod = method_link.chmod(0o600)?
let _module_linked = fs.hardlink(method_link, module_link)?
let _module_chmod = fs.chmod(module_link, 0o600)?
let _copied = source.copy(copied_path)?
let _renamed = copied_path.rename(renamed_path, overwrite: true)?
let _touched = stamp_path.touch(create: true)?
let _empty_made = empty_dir.mkdir()?
let _empty_removed = empty_dir.remove_dir()?
let _unlink_seed = source.hardlink(unlink_path)?
let _unlinked = unlink_path.unlink()?
let _truncate_seed = fs.write(truncate_path, "abcdef")?
let _truncated = truncate_path.truncate(3)?
let parsed_path = Path.parse_bytes(b"byte/path")?
let lines = module_link.lines()?.collect()
let config = ini.decode("""global: root
[server]
host = local
""")?
let encoded = ini.encode({global: "root", server: {host: "local"}})?
let _ini_written = ini.write(ini_path, {global: "written", server: {host: "compact"}})?
let written_config = ini.decode(ini_path.read_text()?)?
let read_config = ini.read(ini_path)?
let env_value = env("XSH_COMPACT_REGISTRY_ENV")?
let checked_record = record.require({name: "pkg", version: "1", extra: 1}, {name: "Str"}, optional: {version: "Str"})?
let lookup = mime.lookup_ext("json") ?? {mime: "missing", exts: [""]}
let path_lookup = mime.lookup_path(p"archive.tar.gz") ?? {mime: "missing", exts: [""]}
let missing = mime.lookup_ext("definitelymissingxsh") ?? {mime: "missing", exts: [""]}
let info = mime.parse("application/json")?
let truncated_text = truncate_path.read_text()?
let path_ops = f"${renamed_path.exists()?} ${stamp_path.exists()?} ${empty_dir.exists()?} ${unlink_path.exists()?} ${parsed_path.display()}"
let rendered = f"${lines[0]} ${config.global} ${config.server.host} ${encoded.contains("[server]")} ${written_config.global} ${written_config.server.host} ${read_config.global} ${read_config.server.host} ${env_value} ${checked_record.name} ${checked_record.version} ${lookup.mime} ${path_lookup.mime} ${missing.mime} ${info.type} ${truncated_text} ${path_ops}"
let _result = fs.write(result_path, rendered)?
"#,
    );
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-path-registry-modules.xsh", &source);
    let parsed = Parser::parse_source_arena_only(source_id, &source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let _constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, &source);
    assert_eq!(
        _constructed.top_level_blockers,
        [0; 11],
        "top={}/{} expr_kinds={:?} call_blockers={:?} callees={:?}",
        _constructed.constructed_top_level_statements,
        _constructed.top_level_statements,
        _constructed.top_level_binding_expression_expr_kinds,
        _constructed.top_level_binding_expression_call_blockers,
        _constructed.top_level_binding_expression_call_callees
    );
    assert_eq!(
        _constructed.constructed_top_level_statements,
        _constructed.top_level_statements
    );

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources)
        .with_env_var("XSH_COMPACT_REGISTRY_ENV", "compact-env");
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("path methods and registry modules should stay compact-only");

    assert_eq!(
        output.status, 0,
        "stdout={:?} stderr={:?} diagnostics={:?} traceback={:?}",
        output.stdout, output.stderr, output.diagnostics, output.traceback
    );
    assert!(
        root.join("method-link.txt").exists(),
        "method link missing; root entries={:?}",
        fs::read_dir(&root)
            .expect("read temp root")
            .map(|entry| entry.expect("dir entry").file_name())
            .collect::<Vec<_>>()
    );
    assert!(
        root.join("module-link.txt").exists(),
        "module link missing; root entries={:?}",
        fs::read_dir(&root)
            .expect("read temp root")
            .map(|entry| entry.expect("dir entry").file_name())
            .collect::<Vec<_>>()
    );
    let result = fs::read_to_string(root.join("result.txt")).unwrap_or_else(|error| {
        panic!(
            "read result failed: {error}; root entries={:?}",
            fs::read_dir(&root)
                .expect("read temp root")
                .map(|entry| entry.expect("dir entry").file_name())
                .collect::<Vec<_>>()
        )
    });
    assert_eq!(
        result,
        "alpha root local true written compact written compact compact-env pkg 1 application/json application/tar+gzip missing application/json abc true true false false byte/path"
    );
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn compact_lowered_only_handles_result_match_patterns() {
    let source = r#"
error CompactResultError = Failed(message: Str)

proc describe(value: Result[Str]) -> Str {
  match value {
    Ok(text) => return text
    Err(err) => return err.message
  }
}

let ok_value: Result[Str] = Ok("yes")
let err_value: Result[Str] = Err(CompactResultError.Failed(message: "no"))
print ${describe(ok_value)} ${describe(err_value)}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-result-match-patterns.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
    assert_eq!(
        constructed.function_blockers, [0; 6],
        "{:?}",
        constructed.function_body_tail_call_callees
    );

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("Result match patterns should stay compact-only");

    assert_eq!(output.stdout, b"yes no\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_captures_simple_run_text() {
    let source = r#"
let label = "alpha"
let items = ["one", "two words"]
let out = run.text XSH_LABEL=(label) sh -c r"""printf '%s|%s|%s|%s' "$XSH_LABEL" "$1" "$2" "$3";""" sh prefix @items ?
print ${out}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-run-text.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
    assert_eq!(
        constructed.constructed_top_level_statements, 4,
        "{:?}",
        constructed.top_level_blockers
    );

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("simple run.text captures should stay compact-only");

    assert_eq!(output.stdout, b"alpha|prefix|one|two words\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_captures_run_text_inside_proc_body() {
    run_with_big_stack(|| {
        let source = r#"
    proc capture() [process, error] -> Result[Str] {
      let out = run.text printf "%s" "body" ?
      return Ok(out)
    }

    let value = capture()?
    print ${value}
    "#;
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("compact-run-text-body.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let constructed =
            probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
        assert!(
            constructed.constructed_functions >= 1,
            "{:?}",
            constructed.function_blockers
        );
        assert_eq!(
            constructed.constructed_top_level_statements, 2,
            "{:?}",
            constructed.top_level_blockers
        );

        let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
        let output = evaluator
            .eval_compact_lowered_only(&parsed.arena, source_id)
            .expect("run.text captures inside proc bodies should stay compact-only");

        assert_eq!(output.stdout, b"body\n");
        assert_eq!(output.status, 0);
        assert!(output.traceback.is_none());
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    });
}

#[test]
fn compact_lowered_only_returns_run_text_result_from_proc_body() {
    let source = r#"
proc capture() [process, error] -> Result[Str] {
  return run.text printf "%s" "returned"
}

let value = capture()?
print ${value}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-run-text-return.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
    assert!(
        constructed.constructed_functions >= 1,
        "{:?}",
        constructed.function_blockers
    );
    assert_eq!(
        constructed.constructed_top_level_statements, 2,
        "{:?}",
        constructed.top_level_blockers
    );

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("returned run.text captures should stay compact-only");

    assert_eq!(output.stdout, b"returned\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_runs_plain_run_commands() {
    let source = r#"
proc emit() [process, error] -> Result[Unit] {
  run sh -c "true" ?
  return Ok()
}

run sh -c "true" ?
emit()?
print done
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-run-command.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
    assert!(
        constructed.constructed_functions >= 1,
        "{:?}",
        constructed.function_blockers
    );
    assert_eq!(
        constructed.constructed_top_level_statements, 3,
        "{:?}",
        constructed.top_level_blockers
    );

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("plain run commands should stay compact-only");

    assert_eq!(output.stdout, b"done\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_runs_process_command_plans() {
    let source = r#"
proc run_command(command: Command) [process] -> Result[Status] {
  return process.run(command)
}

let command: Command = process.command_argv(
  "/bin/sh",
  ["sh", "-c", "test \"$XSH_COMPACT_PROCESS\" = yes"],
  cwd: ".",
  env: {XSH_COMPACT_PROCESS: "yes"},
  timeout: 5s,
)
let status = run_command(command)?
let spawned = process.spawn(process.command_argv("/bin/sh", ["sh", "-c", "true"]))?
print ${status.exited_with(0)} ${spawned.pid > 0}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-process-command-plan.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
    assert!(
        constructed.constructed_functions >= 1,
        "{:?}",
        constructed.function_blockers
    );
    assert_eq!(
        constructed.constructed_top_level_statements, 4,
        "{:?}",
        constructed.top_level_blockers
    );

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("process command plans should stay compact-only");

    assert_eq!(output.stdout, b"true true\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_handles_spawn_wait_and_cancel_handles() {
    let source = r#"
proc run_wait(command: Command) [process] -> Result[Status] {
  let h: ProcessHandle = spawn command?
  return wait h?
}

proc cancel_now(command: Command) [process] -> Result[Unit] {
  let h: ProcessHandle = spawn command?
  h.cancel(signal: "TERM", kill_after: 0ms)?
  return Ok()
}

proc process_controls() [process] -> Result[Unit] {
  let child = process.spawn(process.command_argv("/bin/sh", ["sh", "-c", "sleep 5"]))?
  process.kill(child.pid, signal: "TERM")?
  let any_handle = spawn run true ?
  let any = process.wait_any([any_handle])?
  let ready_one = spawn run true ?
  let ready_two = spawn run true ?
  let ready = process.wait_ready([ready_one, ready_two])?
  if any.index != 0 or ready.len() < 1 {
    return Err(Error("process-control", "unexpected process wait result"))
  }
  return Ok()
}

let status = run_wait(process.command_argv("/bin/sh", ["sh", "-c", "exit 0"]))?
let _ = cancel_now(process.command_argv("/bin/sh", ["sh", "-c", "sleep 5"]))?
let _ = process_controls()?
print ${status.exited_with(0)} cancelled controls
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-spawn-wait-cancel.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
    assert_eq!(
        constructed.function_blockers, [0; 6],
        "{:?}",
        constructed.function_body_tail_stmt_kinds
    );

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("spawn/wait/cancel handles should stay compact-only");

    assert_eq!(output.stdout, b"true cancelled controls\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_handles_float_literals_methods_and_tags() {
    run_with_big_stack(|| {
        let source = r#"
    type Json = JNum(Float) | JInt(Int)

    pure half(n: Int) -> Float {
      return n.float() / 2.0
    }

    pure whole(value: Float) -> Result[Int] {
      return value.sqrt().floor()
    }

    pure boxed(value: Float) -> Json {
      return JNum(value)
    }

    let value = whole(half(9))?
    print ${value}
    "#;
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("float-lowered.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let constructed =
            probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
        assert!(
            constructed.constructed_functions >= 3,
            "{:?}",
            constructed.function_blockers
        );
        assert_eq!(
            constructed.constructed_top_level_statements, 2,
            "{:?}",
            constructed.top_level_blockers
        );

        let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
        let output = evaluator
            .eval_compact_lowered_only(&parsed.arena, source_id)
            .expect("Float literals and methods should stay compact-only");

        assert_eq!(output.stdout, b"2\n");
        assert_eq!(output.status, 0);
        assert!(output.traceback.is_none());
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    });
}

#[test]
fn compact_lowered_only_handles_duration_literals_and_time_calls() {
    let source = r#"
let lit = 1ms
let zero = time.millis(-4)
let one = time.seconds(1)
let slept = time.sleep(0ms)
let year = time.format(0, "%Y", utc: true)
let compact = time.duration_compact(65)
print ${lit}
print ${zero}
print ${one}
print ${year}
print ${compact}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("duration-time-lowered.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
    assert_eq!(
        constructed.constructed_top_level_statements, 11,
        "{:?}",
        constructed.top_level_blockers
    );

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("Duration literals and time calls should stay compact-only");

    assert_eq!(output.stdout, b"1ms\n0h\n1s\n1970\n    1:05\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_preserves_dynamic_json_any_flow() {
    let mut out = std::env::temp_dir();
    out.push(format!(
        "xsh-compact-json-any-{}.json",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos()
    ));
    let source = r#"
let out = fp"${args[0]}"
let data = json.decode("{\"name\":\"demo\",\"ok\":true}")?
json.write(out, data, pretty: true)?
let reread = json.read(out)?
print ${reread.name} ${reread.ok}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-json-any.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
    assert_eq!(
        constructed.constructed_top_level_statements, 5,
        "{:?}",
        constructed.top_level_blockers
    );

    let evaluator = Evaluator::new_with_sources(vec![out.display().to_string()], sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("dynamic JSON Any flow should stay compact-only");

    assert_eq!(output.stdout, b"demo true\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let _ = fs::remove_file(out);
}

#[test]
fn compact_lowered_only_handles_typed_require() {
    let source = r#"
type Package = {name: Str, version: Str, tags: List[Str]}

let sample = "{\"name\":\"demo\",\"version\":\"1.0\",\"tags\":[\"alpha\",\"beta\"]}"
let package = json.decode(sample)?.require(Package)?
print ${package.name} ${package.version} ${package.tags.join("|")}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-require.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("typed require should stay compact-only");

    assert_eq!(output.stdout, b"demo 1.0 alpha|beta\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_handles_guarded_loop_control() {
    let source = r#"
var tries = 0
while tries < 4 {
  tries += 1
  if tries == 2 {
    print "two"
    continue
  }
  break when tries == 4
  print $tries
}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-guarded-loop.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
    assert_eq!(
        constructed.constructed_top_level_statements, 2,
        "{:?}",
        constructed.top_level_blockers
    );

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("guarded loop control should stay compact-only");

    assert_eq!(output.stdout, b"1\ntwo\n3\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_covers_fs_write_mkdir_remove() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "xsh-compact-fs-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos()
    ));
    let source = r#"
let root = fp"${args[0]}"
let nested = fp"${root}/nested"
let use_parents = true
let ignore_missing = true
root.remove(missing_ok: ignore_missing)?
nested.mkdir(parents: use_parents)?
let note = fp"${nested}/note.txt"
note.write("old")?
note.write_atomic("hello")?
let present = note.exists()?
let meta = note.metadata()?
let size = note.du()?
let executable = note.executable()?
let resolved = note.resolve()?
print ${present} ${meta.kind} ${size} ${executable} ${resolved.name()} ${note.read_text()?.trim()}
root.remove(missing_ok: ignore_missing)?
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-fs-write-mkdir-remove.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let evaluator = Evaluator::new_with_sources(vec![root.display().to_string()], sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("fs and path write/mkdir/remove/exists should stay compact-only");

    assert_eq!(output.stdout, b"true file 5 false note.txt hello\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert!(!root.exists());
}

#[test]
fn compact_lowered_only_runs_implicit_proc_main_with_rest_args() {
    let source = "proc main(...argv: List[Str]) [error] -> Int {
  return argv.len()
}
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-auto-main.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let evaluator =
        Evaluator::new_with_sources(vec!["one".to_string(), "two".to_string()], sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("implicit proc main with rest args should not require compatibility Program");

    assert_eq!(output.status, 2);
    assert!(output.stdout.is_empty());
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty());
}

#[test]
fn compact_lowered_only_runs_default_and_rest_params() {
    let source = r#"
pure label(prefix: Str = "item", ...values: List[Str]) -> Str {
  return prefix + ":" + values.join("|")
}

print ${label()}
print ${label("row", "a", "b")}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-default-rest.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("constant default params and rest args should stay compact-only");

    assert_eq!(output.stdout, b"item:\nrow:a|b\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_runs_explicit_spliced_main_and_path_constructor() {
    let source = r#"
proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let path = Path(argv[0])
  print ${path.name()}
  return Ok()
}

let args = ["alpha.txt"]
main(@args)?
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-explicit-spliced-main.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("explicit spliced main and Path constructor should stay compact-only");

    assert_eq!(output.stdout, b"alpha.txt\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_implicit_main_covers_final_args_call() {
    let source = r#"
proc main(...argv: List[Str]) [error] {
  print ${argv.join("|")}
}

main(@args)?
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-final-main-args.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let evaluator =
        Evaluator::new_with_sources(vec!["alpha".to_string(), "beta".to_string()], sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("final main(@args) should use compact implicit main when the explicit call is not lowered");

    assert_eq!(output.stdout, b"alpha|beta\n");
    assert_eq!(output.status, 0);
    assert!(output.traceback.is_none());
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn compact_lowered_only_lowers_mutually_recursive_procs() {
    let source = r#"
proc even(n: Int) [error] -> Bool {
  if n == 0 {
    return true
  }
  return odd(n - 1)?
}

proc odd(n: Int) [error] -> Bool {
  if n == 0 {
    return false
  }
  return even(n - 1)?
}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-mutual-procs.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let diagnostics = evaluator.install_compact_lowered_program(&parsed.arena, source_id);

    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    assert!(evaluator.lowered_procs.contains_key(&Name::intern("even")));
    assert!(evaluator.lowered_procs.contains_key(&Name::intern("odd")));
}

#[test]
fn compact_lowered_only_lowers_par_map_blocks_with_match_prefix() {
    let source = r#"
let values = [1, 2]
let results = values
  |> par-map { |value|
    var hits = []
    match value {
      1 => {
        hits = hits.push(value)
      }
      _ => {}
    }
    hits
  }
  |> flat-map { |hits| hits }

for result in results {
  print result
}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-par-map-match.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let constructed =
        probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
    assert_eq!(
        constructed.constructed_top_level_statements, 3,
        "{:?}",
        constructed
    );
    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    let diagnostics = evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("compact lowered output");

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.stdout, b"1\n");
}

#[test]
fn compact_install_registers_runtime_tag_and_error_metadata() {
    let source = "type Maybe = None | Some(Int)
error ParseError = Bad(message: Str) : InvalidData | Missing(path: Path)
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("compact-runtime-declarations.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.tag_variants.clear();
    evaluator.error_families.clear();

    evaluator.install_compact_lowered_program(&parsed.arena, source_id);

    assert_eq!(evaluator.tag_variants.get(&Name::intern("None")), Some(&0));
    assert_eq!(evaluator.tag_variants.get(&Name::intern("Some")), Some(&1));
    assert!(
        evaluator
            .error_families
            .contains_key(&Name::intern("ParseError"))
    );
}

#[test]
fn map_empty_constructor_lowers_record_builder() {
    // Every tokei scanner builds `blobs: map.empty()`; before the empty-map
    // constructor lowered, that single call rejected the whole function and
    // forced the scan onto the AST evaluator.
    let source = "type Stats = {count: Int, blobs: Map[Any]}
pure build_stats(count: Int) -> Stats {
  return {count, blobs: map.empty()}
}

pure blob_count(stats: Stats) -> Int {
  return stats.blobs.keys().len()
}

print ${blob_count(build_stats(2))}
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("map-empty-lowered.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    let lowered_names = evaluator.lowered_pures.keys().cloned().collect::<Vec<_>>();
    assert!(
        evaluator
            .lowered_pures
            .contains_key(&Name::intern("build_stats")),
        "build_stats should lower with map.empty(); lowered={lowered_names:?}"
    );

    // The lowered `map.empty()` produces a real empty map at runtime.
    let output = evaluator.eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"0\n");
    assert!(output.traceback.is_none());
}

#[test]
fn block_scoped_rebinding_lowers_and_preserves_outer_binding() {
    // `count` is re-`let` in a sibling scope (the early-return `if` and the
    // body) — the tokei-scanner pattern. Block scoping must let that lower,
    // while a nested shadow must not clobber the outer binding after the block.
    let source = "pure classify(n: Int) -> Int {
  if n < 0 {
let count = 0
return count
  }
  let count = n
  return count
}

pure shadow(c: Bool) -> Int {
  let x = 1
  if c {
let x = 2
if x > 100 {
  return 99
}
  }
  return x
}

print ${classify(5)}
print ${shadow(true)}
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("block-scope-rebind.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    let lowered_names = evaluator.lowered_pures.keys().cloned().collect::<Vec<_>>();
    // Sibling-scope re-binding now lowers (the tokei-scanner pattern).
    assert!(
        evaluator
            .lowered_pures
            .contains_key(&Name::intern("classify")),
        "sibling-scope re-binding should lower; lowered={lowered_names:?}"
    );

    // classify(5)=5; shadow(true) must return the OUTER x (1), not the shadow
    // (2) — block scoping is correct.
    let output = evaluator.eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"5\n1\n");
    assert!(output.traceback.is_none());
}

#[test]
fn bytes_slice_utf8_and_list_bytes_lower() {
    // `Bytes.slice`/`Bytes.utf8` and `List[Bytes]` locals are tokei-scanner
    // building blocks; they must lower and match the AST method semantics.
    let source = "pure tail(text: Bytes) -> Bytes {
  return text.slice(3)
}

pure decode(text: Bytes) -> Str {
  return text.utf8() ?? \"bad\"
}

pure collect() -> Int {
  var parts: List[Bytes] = []
  parts = parts.push(b\"x\")
  return parts.len()
}

print ${tail(b\"abcdef\").len()}
print ${decode(b\"hi\")}
print ${decode(b\"\\xff\")}
print ${collect()}
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("bytes-method-coverage.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    let lowered_names = evaluator.lowered_pures.keys().cloned().collect::<Vec<_>>();
    for name in ["tail", "decode", "collect"] {
        assert!(
            evaluator.lowered_pures.contains_key(&Name::intern(name)),
            "{name} should lower; lowered={lowered_names:?}"
        );
    }

    // slice(3) of "abcdef" -> "def" (len 3); utf8 Ok -> "hi", invalid -> "bad"; list len 1.
    let output = evaluator.eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"3\nhi\nbad\n1\n");
    assert!(output.traceback.is_none());
}

fn lowered_body_has_for_str_lines(statements: &[LoweredStmt]) -> bool {
    statements.iter().any(|stmt| match stmt {
        LoweredStmt::ForStrLines { .. } => true,
        LoweredStmt::If {
            branches,
            else_body,
        } => {
            branches
                .iter()
                .any(|(_, body)| lowered_body_has_for_str_lines(body))
                || else_body
                    .as_ref()
                    .is_some_and(|body| lowered_body_has_for_str_lines(body))
        }
        LoweredStmt::IfBool {
            branches,
            else_body,
        } => {
            branches
                .iter()
                .any(|(_, body)| lowered_body_has_for_str_lines(body))
                || else_body
                    .as_ref()
                    .is_some_and(|body| lowered_body_has_for_str_lines(body))
        }
        LoweredStmt::While { body, .. }
        | LoweredStmt::WhileBool { body, .. }
        | LoweredStmt::For { body, .. }
        | LoweredStmt::Cd { body, .. }
        | LoweredStmt::Env { body, .. } => lowered_body_has_for_str_lines(body),
        LoweredStmt::Match { arms, .. } => arms
            .iter()
            .any(|(_, _, body)| lowered_body_has_for_str_lines(body)),
        LoweredStmt::StrMatch { arms, fallback, .. } => {
            arms.values()
                .any(|body| lowered_body_has_for_str_lines(body))
                || fallback
                    .as_ref()
                    .is_some_and(|body| lowered_body_has_for_str_lines(body))
        }
        LoweredStmt::TagMatch { arms, fallback, .. } => {
            arms.values()
                .any(|body| lowered_body_has_for_str_lines(body))
                || fallback
                    .as_ref()
                    .is_some_and(|body| lowered_body_has_for_str_lines(body))
        }
        _ => false,
    })
}

fn lowered_expr_has_str_predicate(expr: &LoweredExpr) -> bool {
    match expr {
        LoweredExpr::StrPredicate { .. } | LoweredExpr::Contains { .. } => true,
        LoweredExpr::Binary { left, right, .. } => {
            lowered_expr_has_str_predicate(left) || lowered_expr_has_str_predicate(right)
        }
        LoweredExpr::IfExpr {
            branches,
            else_value,
            ..
        } => {
            branches.iter().any(|(condition, value)| {
                lowered_expr_has_str_predicate(condition) || lowered_expr_has_str_predicate(value)
            }) || lowered_expr_has_str_predicate(else_value)
        }
        LoweredExpr::MatchExpr { value, arms, .. } => {
            lowered_expr_has_str_predicate(value)
                || arms.iter().any(|(_, guard, value)| {
                    guard.as_ref().is_some_and(lowered_expr_has_str_predicate)
                        || lowered_expr_has_str_predicate(value)
                })
        }
        LoweredExpr::StrMatchExpr {
            value,
            arms,
            fallback,
            ..
        } => {
            lowered_expr_has_str_predicate(value)
                || arms.values().any(lowered_expr_has_str_predicate)
                || fallback
                    .as_ref()
                    .is_some_and(|fallback| lowered_expr_has_str_predicate(fallback))
        }
        LoweredExpr::TagMatchExpr {
            value,
            arms,
            fallback,
            ..
        } => {
            lowered_expr_has_str_predicate(value)
                || arms.values().any(lowered_expr_has_str_predicate)
                || fallback
                    .as_ref()
                    .is_some_and(|fallback| lowered_expr_has_str_predicate(fallback))
        }
        LoweredExpr::ResultFallback { left, right } => {
            lowered_expr_has_str_predicate(left) || lowered_expr_has_str_predicate(right)
        }
        LoweredExpr::FmtString(parts) => parts.iter().any(|part| match part {
            LoweredFmtPart::Text(_) => false,
            LoweredFmtPart::Expr(expr, _, _) => lowered_expr_has_str_predicate(expr),
        }),
        LoweredExpr::PathFmtString { parts, .. } => parts.iter().any(|part| match part {
            LoweredFmtPart::Text(_) => false,
            LoweredFmtPart::Expr(expr, _, _) => lowered_expr_has_str_predicate(expr),
        }),
        LoweredExpr::Record(fields) => fields.iter().any(|field| match field {
            LoweredRecordEntry::Field(_, value) | LoweredRecordEntry::Spread(value) => {
                lowered_expr_has_str_predicate(value)
            }
        }),
        LoweredExpr::Loop { body, .. } => lowered_body_has_str_predicate(body),
        LoweredExpr::Retry { delays, body, .. } => {
            delays.iter().any(lowered_expr_has_str_predicate)
                || lowered_body_has_str_predicate(body)
        }
        LoweredExpr::ProcessCommandBuilder { entries, .. } => {
            entries.iter().any(|entry| match entry {
                LoweredProcessCommandBuilderEntry::Field { value, .. } => {
                    lowered_expr_has_str_predicate(value)
                }
                LoweredProcessCommandBuilderEntry::Run {
                    timeout, cpu_max, ..
                } => {
                    timeout.as_ref().is_some_and(lowered_expr_has_str_predicate)
                        || cpu_max.as_ref().is_some_and(lowered_expr_has_str_predicate)
                }
            })
        }
        LoweredExpr::List(items) => items.iter().any(lowered_expr_has_str_predicate),
        LoweredExpr::StrByteLen { .. } | LoweredExpr::StrByteAt { .. } => false,
        LoweredExpr::EmptyMap => false,
        LoweredExpr::Range { start, end, .. } => {
            lowered_expr_has_str_predicate(start) || lowered_expr_has_str_predicate(end)
        }
        LoweredExpr::Tag { fields, .. } => fields.iter().any(lowered_expr_has_str_predicate),
        LoweredExpr::ListComp {
            value,
            iter,
            condition,
            ..
        } => {
            lowered_expr_has_str_predicate(value)
                || lowered_expr_has_str_predicate(iter)
                || condition
                    .as_ref()
                    .is_some_and(|condition| lowered_expr_has_str_predicate(condition))
        }
        LoweredExpr::MapComp {
            key,
            value,
            iter,
            condition,
            ..
        } => {
            lowered_expr_has_str_predicate(key)
                || lowered_expr_has_str_predicate(value)
                || lowered_expr_has_str_predicate(iter)
                || condition
                    .as_ref()
                    .is_some_and(|condition| lowered_expr_has_str_predicate(condition))
        }
        LoweredExpr::ListPipeline { input, stages, .. } => {
            lowered_expr_has_str_predicate(input)
                || stages.iter().any(|stage| match stage {
                    LoweredPipelineStage::Where { predicate, .. } => {
                        lowered_expr_has_str_predicate(predicate)
                    }
                    LoweredPipelineStage::Map { value, .. } => {
                        lowered_expr_has_str_predicate(value)
                    }
                    LoweredPipelineStage::MapBlock { body, value, .. } => {
                        lowered_body_has_str_predicate(body)
                            || lowered_expr_has_str_predicate(value)
                    }
                    LoweredPipelineStage::FlatMap { value, .. } => {
                        lowered_expr_has_str_predicate(value)
                    }
                    LoweredPipelineStage::SortBy { key, .. } => lowered_expr_has_str_predicate(key),
                    LoweredPipelineStage::GroupBy { key, .. } => {
                        lowered_expr_has_str_predicate(key)
                    }
                    LoweredPipelineStage::CountBy { key, .. } => {
                        lowered_expr_has_str_predicate(key)
                    }
                    LoweredPipelineStage::UniqueBy { key, .. } => {
                        lowered_expr_has_str_predicate(key)
                    }
                    LoweredPipelineStage::Any { predicate, .. }
                    | LoweredPipelineStage::All { predicate, .. } => {
                        lowered_expr_has_str_predicate(predicate)
                    }
                    LoweredPipelineStage::Take(value) | LoweredPipelineStage::Drop(value) => {
                        lowered_expr_has_str_predicate(value)
                    }
                    LoweredPipelineStage::Sort { descending } => descending
                        .as_ref()
                        .is_some_and(lowered_expr_has_str_predicate),
                    _ => false,
                })
        }
        LoweredExpr::Field { base, .. } => lowered_expr_has_str_predicate(base),
        LoweredExpr::Index { base, index, .. } => {
            lowered_expr_has_str_predicate(base) || lowered_expr_has_str_predicate(index)
        }
        LoweredExpr::Method { receiver, args, .. } => {
            lowered_expr_has_str_predicate(receiver)
                || args.iter().any(lowered_expr_has_str_predicate)
        }
        LoweredExpr::RegexCompile { pattern, .. } => lowered_expr_has_str_predicate(pattern),
        LoweredExpr::Require { value, .. } => lowered_expr_has_str_predicate(value),
        LoweredExpr::PathFrom { value, .. } => lowered_expr_has_str_predicate(value),
        LoweredExpr::RunCapture {
            target,
            args,
            env,
            redirections,
            ..
        } => {
            lowered_run_arg_has_str_predicate(target)
                || args.iter().any(lowered_run_arg_has_str_predicate)
                || env
                    .iter()
                    .any(|assignment| lowered_run_arg_has_str_predicate(&assignment.value))
                || redirections
                    .iter()
                    .any(|redirection| lowered_run_arg_has_str_predicate(&redirection.target))
        }
        LoweredExpr::SpawnRun {
            target, args, env, ..
        } => {
            lowered_run_arg_has_str_predicate(target)
                || args.iter().any(lowered_run_arg_has_str_predicate)
                || env
                    .iter()
                    .any(|assignment| lowered_run_arg_has_str_predicate(&assignment.value))
        }
        LoweredExpr::SpawnCommand { command, .. } => lowered_expr_has_str_predicate(command),
        LoweredExpr::Wait { target, .. } => lowered_expr_has_str_predicate(target),
        LoweredExpr::FsList {
            path,
            stat,
            ordered,
            ..
        } => {
            lowered_expr_has_str_predicate(path)
                || stat
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
                || ordered
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
        }
        LoweredExpr::FsFiles { root, .. } => lowered_expr_has_str_predicate(root),
        LoweredExpr::FsWalk { root, .. } => lowered_expr_has_str_predicate(root),
        LoweredExpr::FsTempDir { .. } => false,
        LoweredExpr::FsWrite { path, data, .. } => {
            lowered_expr_has_str_predicate(path) || lowered_expr_has_str_predicate(data)
        }
        LoweredExpr::FsMkdir { path, parents, .. } => {
            lowered_expr_has_str_predicate(path)
                || parents
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
        }
        LoweredExpr::FsRemove {
            path, missing_ok, ..
        } => {
            lowered_expr_has_str_predicate(path)
                || missing_ok
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
        }
        LoweredExpr::FsCloseRoot { root, .. } => lowered_expr_has_str_predicate(root),
        LoweredExpr::FsRootPath { root, .. } => lowered_expr_has_str_predicate(root),
        LoweredExpr::PathReadText { path, .. } => lowered_expr_has_str_predicate(path),
        LoweredExpr::PathReadBytes { path, .. } => lowered_expr_has_str_predicate(path),
        LoweredExpr::PathExists { path, .. } => lowered_expr_has_str_predicate(path),
        LoweredExpr::PathExecutable { path, .. } => lowered_expr_has_str_predicate(path),
        LoweredExpr::PathDu { path, .. } => lowered_expr_has_str_predicate(path),
        LoweredExpr::PathMetadata { path, .. } => lowered_expr_has_str_predicate(path),
        LoweredExpr::PathReadlink { path, .. } => lowered_expr_has_str_predicate(path),
        LoweredExpr::PathResolve { path, .. } => lowered_expr_has_str_predicate(path),
        LoweredExpr::PathWrite { path, data, .. } => {
            lowered_expr_has_str_predicate(path) || lowered_expr_has_str_predicate(data)
        }
        LoweredExpr::PathMkdir { path, parents, .. } => {
            lowered_expr_has_str_predicate(path)
                || parents
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
        }
        LoweredExpr::PathRemove {
            path, missing_ok, ..
        } => {
            lowered_expr_has_str_predicate(path)
                || missing_ok
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
        }
        LoweredExpr::JsonEncode { value, .. } => lowered_expr_has_str_predicate(value),
        LoweredExpr::ArchiveTarCreate {
            path,
            root,
            entries,
            compression,
            overwrite,
            ..
        } => {
            lowered_expr_has_str_predicate(path)
                || lowered_expr_has_str_predicate(root)
                || lowered_expr_has_str_predicate(entries)
                || compression
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
                || overwrite
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
        }
        LoweredExpr::ArchiveTarList { path, .. } => lowered_expr_has_str_predicate(path),
        LoweredExpr::ArchiveTarExtract { path, dest, .. } => {
            lowered_expr_has_str_predicate(path) || lowered_expr_has_str_predicate(dest)
        }
        LoweredExpr::HashVerifyFile { path, expected, .. } => {
            lowered_expr_has_str_predicate(path) || lowered_expr_has_str_predicate(expected)
        }
        LoweredExpr::ModuleCall { args, .. } => args.iter().any(lowered_expr_has_str_predicate),
        LoweredExpr::ProcessCommandArgv {
            target,
            argv,
            cwd,
            env,
            timeout,
            detach,
            new_session,
            ignore_hup,
            cpu_max,
            ..
        } => {
            lowered_expr_has_str_predicate(target)
                || lowered_expr_has_str_predicate(argv)
                || cwd
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
                || env
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
                || timeout
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
                || detach
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
                || new_session
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
                || ignore_hup
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
                || cpu_max
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
        }
        LoweredExpr::Abort { status, force, .. } => {
            lowered_expr_has_str_predicate(status)
                || force
                    .as_ref()
                    .is_some_and(|value| lowered_expr_has_str_predicate(value))
        }
        LoweredExpr::Ok(value) | LoweredExpr::Err(value) | LoweredExpr::Try(value) => {
            lowered_expr_has_str_predicate(value)
        }
        LoweredExpr::Call { args, .. } | LoweredExpr::SelfCall { args, .. } => {
            args.iter().any(|arg| match arg {
                LoweredCallArg::Single(value) | LoweredCallArg::Splice(value) => {
                    lowered_expr_has_str_predicate(value)
                }
            })
        }
        LoweredExpr::DynamicCall { callee, args, .. } => {
            lowered_expr_has_str_predicate(callee)
                || args.iter().any(|arg| match arg {
                    LoweredCallArg::Single(value) | LoweredCallArg::Splice(value) => {
                        lowered_expr_has_str_predicate(value)
                    }
                })
        }
        LoweredExpr::BytesConcat { arg, .. } => lowered_expr_has_str_predicate(arg),
        LoweredExpr::Slice {
            base, start, end, ..
        } => {
            lowered_expr_has_str_predicate(base)
                || start
                    .as_ref()
                    .is_some_and(|start| lowered_expr_has_str_predicate(start))
                || end
                    .as_ref()
                    .is_some_and(|end| lowered_expr_has_str_predicate(end))
        }
        LoweredExpr::RunPipeline { segments, .. } => segments.iter().any(|segment| {
            lowered_run_arg_has_str_predicate(&segment.target)
                || segment.args.iter().any(lowered_run_arg_has_str_predicate)
        }),
        LoweredExpr::Null
        | LoweredExpr::Unit
        | LoweredExpr::Int(_)
        | LoweredExpr::Float(_)
        | LoweredExpr::Duration(_)
        | LoweredExpr::Bool(_)
        | LoweredExpr::Str(_)
        | LoweredExpr::Bytes(_)
        | LoweredExpr::Path(_)
        | LoweredExpr::Glob { .. }
        | LoweredExpr::LastStatus { .. }
        | LoweredExpr::FunctionRef { .. }
        | LoweredExpr::Param(_)
        | LoweredExpr::Error(_) => false,
    }
}

fn lowered_run_arg_has_str_predicate(arg: &crate::runtime::eval::LoweredRunArg) -> bool {
    match &arg.kind {
        crate::runtime::eval::LoweredRunArgKind::Single(expr)
        | crate::runtime::eval::LoweredRunArgKind::SingleOrSplice(expr)
        | crate::runtime::eval::LoweredRunArgKind::Splice(expr) => {
            lowered_expr_has_str_predicate(expr)
        }
    }
}

fn lowered_bool_has_str_predicate(expr: &LoweredBoolExpr) -> bool {
    match expr {
        LoweredBoolExpr::StrPredicateSlot { .. }
        | LoweredBoolExpr::ContainsSlot { .. }
        | LoweredBoolExpr::StrContainsSlot { .. }
        | LoweredBoolExpr::TrimEmptySlot { .. }
        | LoweredBoolExpr::TrimStrPredicateSlot { .. }
        | LoweredBoolExpr::LiteralCompareSlot { .. } => true,
        LoweredBoolExpr::And(left, right) | LoweredBoolExpr::Or(left, right) => {
            lowered_bool_has_str_predicate(left) || lowered_bool_has_str_predicate(right)
        }
        LoweredBoolExpr::Not(inner) => lowered_bool_has_str_predicate(inner),
        LoweredBoolExpr::Bool(_)
        | LoweredBoolExpr::Slot(_)
        | LoweredBoolExpr::IntCompare { .. } => false,
    }
}

fn lowered_body_has_str_predicate(statements: &[LoweredStmt]) -> bool {
    statements.iter().any(|stmt| match stmt {
        LoweredStmt::Let { value, .. }
        | LoweredStmt::Assign { value, .. }
        | LoweredStmt::AssignField { value, .. }
        | LoweredStmt::Expr { value, .. }
        | LoweredStmt::Run { value, .. }
        | LoweredStmt::Defer { value, .. }
        | LoweredStmt::Return { value }
        | LoweredStmt::Yield { value } => lowered_expr_has_str_predicate(value),
        LoweredStmt::If {
            branches,
            else_body,
        } => {
            branches.iter().any(|(condition, body)| {
                lowered_expr_has_str_predicate(condition) || lowered_body_has_str_predicate(body)
            }) || else_body
                .as_ref()
                .is_some_and(|body| lowered_body_has_str_predicate(body))
        }
        LoweredStmt::IfBool {
            branches,
            else_body,
        } => {
            branches.iter().any(|(condition, body)| {
                lowered_bool_has_str_predicate(condition) || lowered_body_has_str_predicate(body)
            }) || else_body
                .as_ref()
                .is_some_and(|body| lowered_body_has_str_predicate(body))
        }
        LoweredStmt::While { condition, body } => {
            lowered_expr_has_str_predicate(condition) || lowered_body_has_str_predicate(body)
        }
        LoweredStmt::WhileBool { body, .. }
        | LoweredStmt::For { body, .. }
        | LoweredStmt::ForStrLines { body, .. }
        | LoweredStmt::Cd { body, .. }
        | LoweredStmt::Env { body, .. } => lowered_body_has_str_predicate(body),
        LoweredStmt::Match { value, arms, .. } => {
            lowered_expr_has_str_predicate(value)
                || arms
                    .iter()
                    .any(|(_, _, body)| lowered_body_has_str_predicate(body))
        }
        LoweredStmt::StrMatch {
            value,
            arms,
            fallback,
            ..
        } => {
            lowered_expr_has_str_predicate(value)
                || arms
                    .values()
                    .any(|body| lowered_body_has_str_predicate(body))
                || fallback
                    .as_ref()
                    .is_some_and(|body| lowered_body_has_str_predicate(body))
        }
        LoweredStmt::TagMatch {
            value,
            arms,
            fallback,
            ..
        } => {
            lowered_expr_has_str_predicate(value)
                || arms
                    .values()
                    .any(|body| lowered_body_has_str_predicate(body))
                || fallback
                    .as_ref()
                    .is_some_and(|body| lowered_body_has_str_predicate(body))
        }
        LoweredStmt::Guard {
            value, else_body, ..
        } => lowered_expr_has_str_predicate(value) || lowered_body_has_str_predicate(else_body),
        LoweredStmt::LetRecord { source, .. } => lowered_expr_has_str_predicate(source),
        LoweredStmt::ForRecord { iter, body, .. } => {
            lowered_expr_has_str_predicate(iter) || lowered_body_has_str_predicate(body)
        }
        LoweredStmt::LetInt { .. }
        | LoweredStmt::LetBool { .. }
        | LoweredStmt::AssignInt { .. }
        | LoweredStmt::AssignFieldInt { .. }
        | LoweredStmt::AssignBool { .. }
        | LoweredStmt::Print { .. }
        | LoweredStmt::Proc { .. }
        | LoweredStmt::Loop { .. }
        | LoweredStmt::Break
        | LoweredStmt::BreakValue { .. }
        | LoweredStmt::Continue
        | LoweredStmt::AssignIndex { .. } => false,
    })
}

fn lowered_expr_has_trim_str_predicate(_expr: &LoweredExpr) -> bool {
    false
}

fn lowered_bool_has_trim_str_predicate(expr: &LoweredBoolExpr) -> bool {
    match expr {
        LoweredBoolExpr::TrimStrPredicateSlot { .. } => true,
        LoweredBoolExpr::And(left, right) | LoweredBoolExpr::Or(left, right) => {
            lowered_bool_has_trim_str_predicate(left) || lowered_bool_has_trim_str_predicate(right)
        }
        LoweredBoolExpr::Not(inner) => lowered_bool_has_trim_str_predicate(inner),
        LoweredBoolExpr::Bool(_)
        | LoweredBoolExpr::Slot(_)
        | LoweredBoolExpr::IntCompare { .. }
        | LoweredBoolExpr::StrPredicateSlot { .. }
        | LoweredBoolExpr::ContainsSlot { .. }
        | LoweredBoolExpr::StrContainsSlot { .. }
        | LoweredBoolExpr::TrimEmptySlot { .. }
        | LoweredBoolExpr::LiteralCompareSlot { .. } => false,
    }
}

fn lowered_body_has_trim_str_predicate(statements: &[LoweredStmt]) -> bool {
    statements.iter().any(|stmt| match stmt {
        LoweredStmt::Let { value, .. }
        | LoweredStmt::Assign { value, .. }
        | LoweredStmt::AssignField { value, .. }
        | LoweredStmt::Expr { value, .. }
        | LoweredStmt::Run { value, .. }
        | LoweredStmt::Defer { value, .. }
        | LoweredStmt::Return { value }
        | LoweredStmt::Yield { value } => lowered_expr_has_trim_str_predicate(value),
        LoweredStmt::If {
            branches,
            else_body,
        } => {
            branches.iter().any(|(condition, body)| {
                lowered_expr_has_trim_str_predicate(condition)
                    || lowered_body_has_trim_str_predicate(body)
            }) || else_body
                .as_ref()
                .is_some_and(|body| lowered_body_has_trim_str_predicate(body))
        }
        LoweredStmt::IfBool {
            branches,
            else_body,
        } => {
            branches.iter().any(|(condition, body)| {
                lowered_bool_has_trim_str_predicate(condition)
                    || lowered_body_has_trim_str_predicate(body)
            }) || else_body
                .as_ref()
                .is_some_and(|body| lowered_body_has_trim_str_predicate(body))
        }
        LoweredStmt::While { condition, body } => {
            lowered_expr_has_trim_str_predicate(condition)
                || lowered_body_has_trim_str_predicate(body)
        }
        LoweredStmt::WhileBool { body, .. }
        | LoweredStmt::For { body, .. }
        | LoweredStmt::ForStrLines { body, .. }
        | LoweredStmt::Cd { body, .. }
        | LoweredStmt::Env { body, .. } => lowered_body_has_trim_str_predicate(body),
        LoweredStmt::Match { value, arms, .. } => {
            lowered_expr_has_trim_str_predicate(value)
                || arms
                    .iter()
                    .any(|(_, _, body)| lowered_body_has_trim_str_predicate(body))
        }
        LoweredStmt::StrMatch {
            value,
            arms,
            fallback,
            ..
        } => {
            lowered_expr_has_trim_str_predicate(value)
                || arms
                    .values()
                    .any(|body| lowered_body_has_trim_str_predicate(body))
                || fallback
                    .as_ref()
                    .is_some_and(|body| lowered_body_has_trim_str_predicate(body))
        }
        LoweredStmt::TagMatch {
            value,
            arms,
            fallback,
            ..
        } => {
            lowered_expr_has_trim_str_predicate(value)
                || arms
                    .values()
                    .any(|body| lowered_body_has_trim_str_predicate(body))
                || fallback
                    .as_ref()
                    .is_some_and(|body| lowered_body_has_trim_str_predicate(body))
        }
        LoweredStmt::Guard {
            value, else_body, ..
        } => {
            lowered_expr_has_trim_str_predicate(value)
                || lowered_body_has_trim_str_predicate(else_body)
        }
        LoweredStmt::LetRecord { source, .. } => lowered_expr_has_trim_str_predicate(source),
        LoweredStmt::ForRecord { iter, body, .. } => {
            lowered_expr_has_trim_str_predicate(iter) || lowered_body_has_trim_str_predicate(body)
        }
        LoweredStmt::LetInt { .. }
        | LoweredStmt::LetBool { .. }
        | LoweredStmt::AssignInt { .. }
        | LoweredStmt::AssignFieldInt { .. }
        | LoweredStmt::AssignBool { .. }
        | LoweredStmt::Print { .. }
        | LoweredStmt::Proc { .. }
        | LoweredStmt::Loop { .. }
        | LoweredStmt::Break
        | LoweredStmt::BreakValue { .. }
        | LoweredStmt::Continue
        | LoweredStmt::AssignIndex { .. } => false,
    })
}

fn lowered_int_has_count_lines(expr: &LoweredIntExpr) -> bool {
    match expr {
        LoweredIntExpr::StrCountLinesSlot { .. } => true,
        LoweredIntExpr::Binary { left, right, .. } => {
            lowered_int_has_count_lines(left) || lowered_int_has_count_lines(right)
        }
        LoweredIntExpr::Int(_)
        | LoweredIntExpr::Slot(_)
        | LoweredIntExpr::StrByteLenSlot { .. }
        | LoweredIntExpr::StrByteAtSlot { .. } => false,
    }
}

fn lowered_body_has_count_lines(statements: &[LoweredStmt]) -> bool {
    statements.iter().any(|stmt| match stmt {
        LoweredStmt::LetInt { value, .. }
        | LoweredStmt::AssignInt { value, .. }
        | LoweredStmt::AssignFieldInt { value, .. } => lowered_int_has_count_lines(value),
        LoweredStmt::If {
            branches,
            else_body,
        } => {
            branches
                .iter()
                .any(|(_, body)| lowered_body_has_count_lines(body))
                || else_body
                    .as_ref()
                    .is_some_and(|body| lowered_body_has_count_lines(body))
        }
        LoweredStmt::IfBool {
            branches,
            else_body,
        } => {
            branches
                .iter()
                .any(|(_, body)| lowered_body_has_count_lines(body))
                || else_body
                    .as_ref()
                    .is_some_and(|body| lowered_body_has_count_lines(body))
        }
        LoweredStmt::While { body, .. }
        | LoweredStmt::WhileBool { body, .. }
        | LoweredStmt::For { body, .. }
        | LoweredStmt::ForStrLines { body, .. }
        | LoweredStmt::Cd { body, .. }
        | LoweredStmt::Env { body, .. } => lowered_body_has_count_lines(body),
        LoweredStmt::Match { arms, .. } => arms
            .iter()
            .any(|(_, _, body)| lowered_body_has_count_lines(body)),
        LoweredStmt::StrMatch { arms, fallback, .. } => {
            arms.values().any(|body| lowered_body_has_count_lines(body))
                || fallback
                    .as_ref()
                    .is_some_and(|body| lowered_body_has_count_lines(body))
        }
        LoweredStmt::TagMatch { arms, fallback, .. } => {
            arms.values().any(|body| lowered_body_has_count_lines(body))
                || fallback
                    .as_ref()
                    .is_some_and(|body| lowered_body_has_count_lines(body))
        }
        LoweredStmt::Guard { else_body, .. } => lowered_body_has_count_lines(else_body),
        LoweredStmt::ForRecord { body, .. } => lowered_body_has_count_lines(body),
        LoweredStmt::Let { .. }
        | LoweredStmt::LetBool { .. }
        | LoweredStmt::LetRecord { .. }
        | LoweredStmt::Assign { .. }
        | LoweredStmt::AssignField { .. }
        | LoweredStmt::AssignBool { .. }
        | LoweredStmt::Expr { .. }
        | LoweredStmt::Run { .. }
        | LoweredStmt::Defer { .. }
        | LoweredStmt::Return { .. }
        | LoweredStmt::Yield { .. }
        | LoweredStmt::Print { .. }
        | LoweredStmt::Proc { .. }
        | LoweredStmt::Loop { .. }
        | LoweredStmt::Break
        | LoweredStmt::BreakValue { .. }
        | LoweredStmt::Continue
        | LoweredStmt::AssignIndex { .. } => false,
    })
}

#[test]
fn lowered_count_lines_uses_direct_int_node() {
    let source = "pure score(text: Str, blanks: Int) -> Int {
  let lines = text.count_lines()
  return lines - blanks
}

score(\"a\\nb\\n\", 1)
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("count-lines-lowered-registry.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    let lowered = evaluator
        .lowered_pures
        .get(&Name::intern("score"))
        .expect("score should lower");

    assert!(
        lowered_body_has_count_lines(&lowered.body),
        "body={:?}",
        lowered.body
    );
}

#[test]
fn text_lines_for_loop_uses_streaming_lowered_stmt() {
    let source = "pure count_nonblank(text: Str) -> Int {
  var total = 0

  for line in text.lines() {
if line.trim() != \"\" {
  total += 1
}
  }

  return total
}

count_nonblank(\"a\\n\\nb\\n\")
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("text-lines-for-lowered-registry.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    let lowered = evaluator
        .lowered_pures
        .get(&Name::intern("count_nonblank"))
        .expect("count_nonblank should lower");

    assert!(
        lowered_body_has_for_str_lines(&lowered.body),
        "body={:?}",
        lowered.body
    );
    assert!(
        lowered_body_has_str_predicate(&lowered.body),
        "body={:?}",
        lowered.body
    );
}

#[test]
fn lowered_text_lines_views_preserve_escaped_lines() {
    let source = "pure collect(text: Str) -> Str {
  var lines: List[Str] = []

  for line in text.lines() {
lines = lines.push(line)
  }

  return lines.join(\"|\")
}

print ${collect(\"alpha\\nbeta\\ngamma\\n\")}
";
    let prepared = crate::runtime::bench::prepare_source("text-lines-view-escape.xsh", source);
    let output = crate::runtime::bench::eval_prepared_output(&prepared);
    let stdout = std::str::from_utf8(&output.stdout).expect("stdout is utf-8");

    assert_eq!(output.status, 0);
    assert_eq!(stdout.trim(), "alpha|beta|gamma");
}

#[test]
fn lowered_string_predicates_use_direct_nodes() {
    let source = "pure score(text: Str) -> Int {
  var total = 0

  for line in text.lines() {
let trimmed = line.trim()

if trimmed.starts_with(\"#\") {
  total += 1
} else if line.contains(\"//\") {
  total += 10
} else if ! line.contains(\"skip\") {
  total += 20
} else if trimmed.ends_with(\";\") {
  total += 100
}
  }

  return total
}

score(\"# a\\ncode // note\\nlet x;\\n\")
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("text-predicate-lowered-registry.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    let lowered = evaluator
        .lowered_pures
        .get(&Name::intern("score"))
        .expect("score should lower");

    assert!(
        lowered_body_has_str_predicate(&lowered.body),
        "body={:?}",
        lowered.body
    );
}

#[test]
fn lowered_string_byte_search_preserves_contains_find_and_membership() {
    let source = "pure marker_score(text: Str) -> Int {
  var total = 0

  for line in text.lines() {
if line.contains(\"/\") {
  total += 1
}

if line.contains(\"//\") {
  total += 10
}

if \"/\" in line {
  total += 100
}

if \"☃\" in line {
  total += 1000
}
  }

  if text.find(\"/*\") >= 0 {
total += 10000
  }

  if text.find(\"\", 3) == 3 {
total += 100000
  }

  return total
}

print ${marker_score(\"plain\\n/a\\n//b\\nsnow ☃\\nbody /* mark\\n\")}
";
    let prepared = crate::runtime::bench::prepare_source("string-byte-search-lowered.xsh", source);
    let output = crate::runtime::bench::eval_prepared_output(&prepared);

    assert_eq!(output.status, 0);
    assert_eq!(output.stdout, b"111313\n");
    assert!(output.traceback.is_none());
}

#[test]
fn lowered_bytes_scanner_covers_lines_trim_predicates_and_byte_at() {
    run_with_big_stack(|| {
        // Exercises the lowered Bytes path end to end: `for line in bytes.lines()`,
        // `trim()`, `== b""`, `starts_with`/`ends_with`/`contains`, `byte_at`, and
        // `count_lines`. Mirrors how the tokei showcase scans file content as Bytes.
        let source = "pure byte_marker_score(text: Bytes) -> Int {
      var total = 0

      for line in text.lines() {
    let t = line.trim()

    if t == b\"\" {
      total += 1
    } else if t.starts_with(b\"#\") {
      total += 10
    } else if t.contains(b\"TODO\") {
      total += 100
    } else if t.ends_with(b\";\") {
      total += 1000
    }

    if line.byte_at(0, -1) == 47 {
      total += 10000
    }
      }

      total += text.count_lines() * 1000000
      return total
    }

    print ${byte_marker_score(b\"  # h\\n\\nx;\\n/a TODO\\n\")}
    ";
        let prepared = crate::runtime::bench::prepare_source("bytes-scanner-lowered.xsh", source);
        let output = crate::runtime::bench::eval_prepared_output(&prepared);

        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, b"4011111\n");
        assert!(output.traceback.is_none());
    });
}

#[test]
fn lowered_trim_string_predicates_use_direct_nodes() {
    let source = "pure score(text: Str) -> Int {
  var total = 0

  for line in text.lines() {
if line.trim().starts_with(\"#\") {
  total += 1
} else if line.trim().ends_with(\";\") {
  total += 10
}
  }

  return total
}

score(\"  # a\\nlet x;\\n\")
";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("trim-text-predicate-lowered-registry.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    let lowered = evaluator
        .lowered_pures
        .get(&Name::intern("score"))
        .expect("score should lower");

    assert!(
        lowered_body_has_trim_str_predicate(&lowered.body),
        "body={:?}",
        lowered.body
    );
}

#[test]
fn lowered_trim_empty_preserves_ascii_and_unicode_whitespace() {
    let source = "pure blank_score(text: Str) -> Int {
  var total = 0

  for line in text.lines() {
if line.trim() == \"\" {
  total += 1
}
  }

  return total
}

print ${blank_score(\"  \\n\\t\\n\\u{2003}\\ncode\\n\")}
";
    let prepared = crate::runtime::bench::prepare_source("trim-empty-lowered.xsh", source);
    let output = crate::runtime::bench::eval_prepared_output(&prepared);

    assert_eq!(output.status, 0);
    assert_eq!(output.stdout, b"3\n");
    assert!(output.traceback.is_none());
}

#[test]
fn lowered_trim_predicates_preserve_ascii_and_unicode_whitespace() {
    let source = "pure marker_score(text: Str) -> Int {
  var total = 0

  for line in text.lines() {
if line.trim().starts_with(\"#\") {
  total += 1
}

if line.trim().ends_with(\";\") {
  total += 10
}
  }

  return total
}

print ${marker_score(\"  # ascii\\n\\u{2003}# unicode\\nlet x;  \\nlet y;\\u{2003}\\n\")}
";
    let prepared = crate::runtime::bench::prepare_source("trim-predicate-lowered.xsh", source);
    let output = crate::runtime::bench::eval_prepared_output(&prepared);

    assert_eq!(output.status, 0);
    assert_eq!(output.stdout, b"22\n");
    assert!(output.traceback.is_none());
}

#[test]
fn lowered_self_collection_assignment_preserves_aliases() {
    let source = r#"pure summarize(seed: Map[Int]) -> Str {
  var xs: List[Str] = []
  let old_xs = xs
  xs = xs.push("alpha")
  xs = xs.push("beta")

  var values = seed
  let old_values = values
  values = values.set("alpha", 10)
  values = values.set("beta", 20)

  return f"${old_xs.len()}:${xs.join(",")}:${old_values.keys().len()}:${values.get("alpha", 0) + values.get("beta", 0)}"
}

var seed: Map[Int] = map.empty()
print ${summarize(seed)}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("self-collection-assignment-lowered.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(
        evaluator
            .lowered_pures
            .contains_key(&Name::intern("summarize"))
    );

    let output = Evaluator::new_with_sources(Vec::new(), sources).eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"0:alpha,beta:0:30\n");
    assert!(output.traceback.is_none());
}

#[test]
fn nested_record_field_access_preserves_behavior() {
    let source = r#"let report = {stats: {blanks: 2, code: 3, comments: 5, blobs: map.empty()}, name: "file.xsh"}
let scan = {deep: {blanks: 7, code: 11, comments: 13, blobs: map.empty()}}
print ${report.stats.blanks + report.stats.code + report.stats.comments + scan.deep.code}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("nested-record-field.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let output = Evaluator::new_with_sources(Vec::new(), sources).eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"21\n");
    assert!(output.traceback.is_none());
}

#[test]
fn run_error_constructor_preserves_status() {
    let value = run_error_from_status(crate::runtime::process::ProcessStatus::exited(1));

    let Value::RunError(error) = value else {
        panic!("expected RunError");
    };
    assert_eq!(error.kind, "nonzero-exit");
    assert_eq!(error.status.unwrap().code, Some(1));
}

#[test]
fn compact_install_lowers_module_sibling_calls_as_qualified_functions() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("xsh-compact-module-ir-{stamp}"));
    fs::create_dir_all(&root).expect("create temp module dir");
    fs::write(
        root.join("compact_helper.xsh"),
        "export pure inner(value: Str) -> Str {
  return value + \"!\"
}

export pure outer(value: Str) -> Str {
  return inner(value)
}
",
    )
    .expect("write helper module");
    let script = root.join("main.xsh");
    fs::write(
        &script,
        "use compact_helper

let value = compact_helper.outer(\"ok\")
print ${value}
",
    )
    .expect("write main script");
    let script_text = script.to_string_lossy().into_owned();
    let (sources, parsed) = parse_script(&script_text).expect("parse temp script");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let source_id = parsed
        .arena
        .arena
        .span_source_id
        .expect("loaded arena should have a primary source");
    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.install_compact_lowered_functions(&parsed.arena, source_id);

    assert!(
        evaluator
            .lowered_qualified_pures
            .contains_key(&QualifiedName::new(
                Name::intern("compact_helper"),
                Name::intern("inner")
            ))
    );
    let outer = evaluator
        .lowered_qualified_pures
        .get(&QualifiedName::new(
            Name::intern("compact_helper"),
            Name::intern("outer"),
        ))
        .expect("compact-lowered module outer");
    let [
        LoweredStmt::Return {
            value:
                LoweredExpr::Call {
                    function: LoweredFunctionKey::Qualified(QualifiedName { namespace, member }),
                    ..
                },
        },
    ] = outer.body.as_slice()
    else {
        panic!("outer should return a qualified lowered sibling call");
    };
    assert_eq!(*namespace, Name::intern("compact_helper"));
    assert_eq!(*member, Name::intern("inner"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn compact_top_level_use_imports_loaded_user_module() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("xsh-compact-use-ir-{stamp}"));
    fs::create_dir_all(&root).expect("create temp module dir");
    fs::write(
        root.join("compact_import.xsh"),
        "export pure label(value: Str) -> Str {
  return value + \"!\"
}
",
    )
    .expect("write helper module");
    let script = root.join("main.xsh");
    fs::write(
        &script,
        "use compact_import

let value = compact_import.label(\"ok\")
print ${value}
",
    )
    .expect("write main script");
    let script_text = script.to_string_lossy().into_owned();
    let (sources, parsed) = parse_script(&script_text).expect("parse temp script");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let source_id = parsed
        .arena
        .arena
        .span_source_id
        .expect("loaded arena should have a primary source");
    let mut probe = Evaluator::new_with_sources(Vec::new(), sources.clone());
    probe.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(matches!(
        probe
            .lowered_program
            .statements
            .first()
            .and_then(|stmt| stmt.as_ref())
            .map(|stmt| &stmt.kind),
        Some(LoweredTopLevelKind::Use { .. })
    ));
    assert!(
        probe
            .lowered_program
            .statements
            .iter()
            .all(|stmt| stmt.is_some()),
        "{:?}",
        probe.lowered_program.statements
    );

    let evaluator = Evaluator::new_with_sources(Vec::new(), sources);
    let output = evaluator
        .eval_compact_lowered_only(&parsed.arena, source_id)
        .expect("loaded compact module program should not require compatibility Program");
    assert_eq!(output.stdout, b"ok!\n");
    assert!(output.traceback.is_none());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pure_text_lines_map_block_pipeline_lowers() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("xsh-text-lines-pipeline-ir-{stamp}"));
    fs::create_dir_all(&root).expect("create temp script dir");
    let script = root.join("main.xsh");
    fs::write(
        &script,
        r#"pure normalized(input_text: Str) -> List[Str] {
  let lines = input_text
|> text.lines
|> where .trim() != ""
|> map { |line|
  let fields = line.fields()
  f"${fields[0]}:${fields[1]}"
}

  return lines
}

let text = """10 alpha

20 beta
"""
print ${normalized(text).join(",")}
"#,
    )
    .expect("write temp script");
    let script_text = script.to_string_lossy().into_owned();
    let (sources, parsed) = parse_script(&script_text).expect("parse temp script");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let source_id = parsed
        .arena
        .arena
        .span_source_id
        .expect("loaded arena should have a primary source");
    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(
        evaluator
            .lowered_pures
            .contains_key(&Name::intern("normalized"))
    );

    let output =
        Evaluator::new_with_sources(Vec::new(), sources.clone()).eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"10:alpha,20:beta\n");
    assert!(output.traceback.is_none());

    let traced = Evaluator::new_with_sources(Vec::new(), sources)
        .with_tracing()
        .eval(&parsed.arena, source_id);
    assert_eq!(traced.stdout, b"10:alpha,20:beta\n");
    assert!(traced.traceback.is_none());
    assert!(!traced.trace_events.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pure_group_by_pipeline_lowers_and_preserves_bucket_order() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("xsh-group-by-pipeline-ir-{stamp}"));
    fs::create_dir_all(&root).expect("create temp script dir");
    let script = root.join("main.xsh");
    fs::write(
        &script,
        r#"type Row = {name: Str, group: Str, weight: Int}

pure summarize(rows: List[Row]) -> Str {
  let labels = rows
|> group-by .group
|> sort-by .key
|> map { |bucket|
  f"${bucket.key}:${bucket.items[0].name}:${bucket.items.len()}"
}

  return labels.join("|")
}

let rows = [
  {name: "first-a", group: "a", weight: 2},
  {name: "first-b", group: "b", weight: 1},
  {name: "second-a", group: "a", weight: 3},
  {name: "second-b", group: "b", weight: 4},
]
print ${summarize(rows)}
"#,
    )
    .expect("write temp script");
    let script_text = script.to_string_lossy().into_owned();
    let (sources, parsed) = parse_script(&script_text).expect("parse temp script");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let source_id = parsed
        .arena
        .arena
        .span_source_id
        .expect("loaded arena should have a primary source");
    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(
        evaluator
            .lowered_pures
            .contains_key(&Name::intern("summarize"))
    );

    let output =
        Evaluator::new_with_sources(Vec::new(), sources.clone()).eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"a:first-a:2|b:first-b:2\n");
    assert!(output.traceback.is_none());

    let traced = Evaluator::new_with_sources(Vec::new(), sources)
        .with_tracing()
        .eval(&parsed.arena, source_id);
    assert_eq!(traced.stdout, b"a:first-a:2|b:first-b:2\n");
    assert!(traced.traceback.is_none());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn pure_reverse_and_map_values_methods_lower() {
    let source = r#"pure summarize(counts: Map[Int], label: Str) -> Str {
  let values = counts.values()
  var total = 0

  for value in values {
total += value
  }

  return f"${label.reverse()}:${total}"
}

var counts: Map[Int] = map.empty()
counts = counts.set("beta", 2)
counts = counts.set("alpha", 1)
print ${summarize(counts, "abc")}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("test.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(
        evaluator
            .lowered_pures
            .contains_key(&Name::intern("summarize"))
    );

    let output =
        Evaluator::new_with_sources(Vec::new(), sources.clone()).eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"cba:3\n");
    assert!(output.traceback.is_none());

    let traced = Evaluator::new_with_sources(Vec::new(), sources.clone())
        .with_tracing()
        .eval(&parsed.arena, source_id);
    assert_eq!(traced.stdout, b"cba:3\n");
    assert!(traced.traceback.is_none());
}

#[test]
fn pure_tag_union_constructors_and_match_patterns_lower() {
    let source = r#"type Level =
    Info
  | Warn
  | Error
  | Debug
  | Trace

type Shape =
    Circle(Int)
  | Rect(Int, Int)
  | Point
  | Square(Int)
  | Triangle(Int, Int)

pure level_label(l: Level) -> Str {
  if l == Info {
return "INFO"
  }

  if l == Warn {
return "WARN"
  }

  if l == Error {
return "ERROR"
  }

  if l == Trace {
return "TRACE"
  }

  "DEBUG"
}

pure area(s: Shape) -> Int {
  match s {
Circle(r) => r * r * 3
Rect(w, h) => w * h
Point => 0
Square(side) => side * side
Triangle(base, height) => base * height / 2
  }
}

print ${level_label(Info)} ${level_label(Error)}
print ${area(Circle(4))} ${area(Rect(3, 5))} ${area(Point)}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("test.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(
        evaluator
            .lowered_pures
            .contains_key(&Name::intern("level_label"))
    );
    assert!(evaluator.lowered_pures.contains_key(&Name::intern("area")));

    let output =
        Evaluator::new_with_sources(Vec::new(), sources.clone()).eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"INFO ERROR\n48 15 0\n");
    assert!(output.traceback.is_none());

    let traced = Evaluator::new_with_sources(Vec::new(), sources.clone())
        .with_tracing()
        .eval(&parsed.arena, source_id);
    assert_eq!(traced.stdout, b"INFO ERROR\n48 15 0\n");
    assert!(traced.traceback.is_none());
}

#[test]
fn top_level_control_statements_lower_to_script_ir() {
    let source = r#"
var total: Int = 0
var i: Int = 0
while i < 10 {
  total += i
  i += 1
}
print ${total}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("test.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    let lowered_count = evaluator
        .lowered_program
        .statements
        .iter()
        .filter(|stmt| stmt.is_some())
        .count();
    assert!(
        lowered_count >= 3,
        "expected typed bindings and while statement to lower"
    );

    let output =
        Evaluator::new_with_sources(Vec::new(), sources.clone()).eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"45\n");
    assert!(output.traceback.is_none());
}

#[test]
fn top_level_script_ir_tracks_untyped_literal_bindings() {
    let source = r#"
var total = 0
var i = 0
while i < 10 {
  total += i
  i += 1
}
print ${total}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("test.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(
        evaluator
            .lowered_program
            .statements
            .get(2)
            .is_some_and(Option::is_some),
        "expected untyped literal vars to expose slots for while"
    );

    let output =
        Evaluator::new_with_sources(Vec::new(), sources.clone()).eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"45\n");
    assert!(output.traceback.is_none());
}

#[test]
fn top_level_script_ir_tracks_untyped_plain_pure_returns() {
    let source = r#"
pure seed() -> Int {
  return 1
}

var total = seed()
var i = 0
while i < 9 {
  total += i
  i += 1
}
print ${total}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("test.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(
        evaluator
            .lowered_program
            .statements
            .get(3)
            .is_some_and(Option::is_some),
        "expected plain pure return to expose a top-level slot"
    );

    let output =
        Evaluator::new_with_sources(Vec::new(), sources.clone()).eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"37\n");
    assert!(output.traceback.is_none());
}

#[test]
fn top_level_script_ir_calls_effect_free_proc_bodies() {
    let source = r#"
proc checked(value: Int) [error] -> Result[Int] {
  if value < 0 {
return Err(Error("negative", "negative value"))
  }
  return Ok(value + 1)
}

var total: Int = 0
var i: Int = 0
while i < 4 {
  total += checked(i)?
  i += 1
}
print ${total}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("test.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(
        evaluator
            .lowered_procs
            .contains_key(&Name::intern("checked"))
    );
    assert!(
        evaluator
            .lowered_program
            .statements
            .get(3)
            .is_some_and(Option::is_some),
        "expected top-level loop to lower through the effect-free proc call"
    );

    let output =
        Evaluator::new_with_sources(Vec::new(), sources.clone()).eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"10\n");
    assert!(output.traceback.is_none());
}

#[test]
fn unrestricted_procs_lower_into_script_ir() {
    // The hybrid AST-eval model is gone: everything is consolidated on the arena,
    // so an unrestricted proc (no effect annotation) now lowers into the script IR
    // like any other, rather than staying AST-evaluated.
    let source = r#"
proc checked(value: Int) -> Int {
  return value + 1
}

var total: Int = 0
total += checked(1)
print ${total}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("test.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(
        evaluator
            .lowered_procs
            .contains_key(&Name::intern("checked")),
        "unrestricted procs now lower into the script IR (arena-consolidated)"
    );

    let output =
        Evaluator::new_with_sources(Vec::new(), sources.clone()).eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"2\n");
    assert!(output.traceback.is_none());
}

#[test]
fn top_level_script_ir_is_gated_by_tracing() {
    let source = r#"
var total: Int = 0
var i: Int = 0
while i < 5 {
  total += i
  i += 1
}
print ${total}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("test.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(
        evaluator
            .lowered_program
            .statements
            .iter()
            .any(Option::is_some)
    );

    let output = Evaluator::new_with_sources(Vec::new(), sources.clone())
        .with_tracing()
        .eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"10\n");
    assert!(output.traceback.is_none());
    assert!(!output.trace_events.is_empty());
}

#[test]
fn evaluates_main_proc_with_argv_and_core_commands() {
    let source = r#"
proc main(args: List[Str]) -> Result[Unit] {
  print "hello"
  for arg in args {
eprint ${arg}
  }
  return Ok()
}

main(args)?
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("test.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let output =
        Evaluator::new_with_sources(vec!["one".to_string(), "two".to_string()], sources.clone())
            .with_tracing()
            .eval(&parsed.arena, source_id);

    assert_eq!(output.stdout, b"hello\n");
    assert_eq!(output.stderr, b"one\ntwo\n");
    assert!(output.traceback.is_none());
    assert!(
        output
            .trace_events
            .iter()
            .any(|event| event.kind == TraceKind::ProcEnter)
    );
}

#[test]
fn runtime_assignment_to_let_reports_span() {
    let source = "let x = 1\nx = 2\n";
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("test.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);

    let output = Evaluator::new_with_sources(Vec::new(), sources.clone())
        .with_tracing()
        .eval(&parsed.arena, source_id);

    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == Some("runtime.error"))
    );
    assert!(output.traceback.is_some());
}

#[test]
fn question_in_binding_initializer_returns_from_proc() {
    let source = r#"
proc main(args: List[Str]) -> Result[Unit] {
  let x = Err(Error(kind: "boom", message: "failed")) ?
  print "unreachable"
  return Ok()
}

main(args)?
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("test.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let output = Evaluator::new_with_sources(Vec::new(), sources.clone())
        .with_tracing()
        .eval(&parsed.arena, source_id);

    assert_eq!(output.stdout, b"");
    assert!(output.traceback.is_some());
    assert!(
        output
            .trace_events
            .iter()
            .any(|event| event.kind == TraceKind::ResultPropagate)
    );
}

#[test]
fn path_value_preserves_non_utf8_bytes_and_joins_lexically() {
    let path = PathValue::new(vec![b'a', 0xff, b'b']).expect("path");
    assert_eq!(path.bytes, vec![b'a', 0xff, b'b']);

    let joined = PathValue::from_text("root")
        .unwrap()
        .join_text("child name")
        .unwrap();
    assert_eq!(joined.bytes, b"root/child name");

    let absolute = joined
        .join_path(&PathValue::from_text("/tmp/x").unwrap())
        .unwrap();
    assert_eq!(absolute.bytes, b"/tmp/x");
}

#[test]
fn argv_conversion_rejects_nul_list_without_splice_and_bytes() {
    let span = Span::new(SourceId::new(0), 0, 1);

    let nul = value_to_argv_bytes(Value::Str("a\0b".into()), span).unwrap_err();
    assert_eq!(nul.kind, "nul-argv");

    let list = value_to_argv_bytes(Value::List(vec![Value::Str("a".into())]), span).unwrap_err();
    assert_eq!(list.kind, "argv-conversion");

    let bytes = value_to_argv_bytes(Value::Bytes(vec![b'a']), span).unwrap_err();
    assert_eq!(bytes.kind, "argv-conversion");
}

#[test]
fn lowered_method_names_have_no_duplicates() {
    use rustc_hash::FxHashSet;
    let mut seen = FxHashSet::default();
    for name in LOWERED_METHOD_NAMES {
        assert!(seen.insert(*name), "duplicate lowered method name: {name}");
    }
}

#[test]
fn lowered_value_matches_accepts_str_and_bytes_including_views() {
    // Regression guard: `lowered_value_matches` is a hand-maintained type↔value
    // table; it once omitted `Bytes`/`BytesView` entirely, which only surfaced as
    // a runtime "expected Bytes, found Bytes" once byte scanners began to lower.
    use std::sync::Arc;

    let text: Arc<str> = Arc::from("hi");
    assert!(lowered_value_matches(
        LoweredType::Str,
        &LoweredValue::Str(text.clone())
    ));
    assert!(lowered_value_matches(
        LoweredType::Str,
        &LoweredValue::StrView(LoweredStrView::new(text, 0, 2))
    ));

    let bytes: Arc<[u8]> = Arc::from(&b"hi"[..]);
    assert!(lowered_value_matches(
        LoweredType::Bytes,
        &LoweredValue::Bytes(bytes.clone())
    ));
    assert!(lowered_value_matches(
        LoweredType::Bytes,
        &LoweredValue::BytesView(LoweredBytesView::new(bytes, 0, 2))
    ));
}

#[test]
fn bytes_concat_lowers_and_matches_ast() {
    run_with_big_stack(|| {
        // `bytes.concat(<List[Bytes]>)` is a builtin constructor; without lowering it,
        // `join_lines` (and the tokei scanner cluster that calls it) falls back to AST.
        let source = r#"pure join_lines(lines: List[Bytes]) -> Bytes {
      var parts: List[Bytes] = []
      var first = true

      for line in lines {
        if ! first {
          parts = parts.push(b"\n")
        }

        parts = parts.push(line)
        first = false
      }

      return bytes.concat(parts)
    }

    let lines: List[Bytes] = [b"alpha", b"beta", b"gamma"]
    print ${join_lines(lines).utf8() ?? "ERR"}
    "#;
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("bytes-concat-lowered.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

        let checked = {
            let sid = parsed
                .arena
                .arena
                .span_source_id
                .expect("loaded arena should have a primary source");
            let text = sources
                .get(sid)
                .map(|s| s.text().to_string())
                .unwrap_or_default();
            Checker::check_arena(&parsed.arena, &text)
        };
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator.install_compact_lowered_program(&parsed.arena, source_id);
        assert!(
            evaluator
                .lowered_pures
                .contains_key(&Name::intern("join_lines")),
            "join_lines should lower once bytes.concat lowers"
        );

        let output =
            Evaluator::new_with_sources(Vec::new(), sources).eval(&parsed.arena, source_id);
        assert_eq!(output.stdout, b"alpha\nbeta\ngamma\n");
        assert!(output.traceback.is_none());
    });
}

#[test]
fn mutually_recursive_pures_colower_atomically() {
    // The single-candidate fixpoint cannot bootstrap a cycle; SCC co-lowering must.
    let source = r#"pure is_even(n: Int) -> Bool {
  if n == 0 {
    return true
  }

  return is_odd(n - 1)
}

pure is_odd(n: Int) -> Bool {
  if n == 0 {
    return false
  }

  return is_even(n - 1)
}

print ${is_even(10)}:${is_odd(7)}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("mutual-recursion-lowered.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(
        evaluator
            .lowered_pures
            .contains_key(&Name::intern("is_even"))
            && evaluator
                .lowered_pures
                .contains_key(&Name::intern("is_odd")),
        "both members of the recursion cycle should co-lower"
    );

    let output = Evaluator::new_with_sources(Vec::new(), sources).eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"true:true\n");
    assert!(output.traceback.is_none());
}

#[test]
fn statement_match_arms_relet_sibling_names() {
    // Sibling statement-match arms each re-`let` the same name; the arm bodies must
    // be block-scoped (like if/else) so the re-binding does not collide. This is the
    // shape the tokei `count_markdown` fence dispatch needs.
    let source = r#"type Tag = TagA | TagB | TagC

pure pick(tag: Tag) -> Int {
  var result = 0

  match tag {
    TagA => {
      let value = 1
      result = value + 10
    }
    TagB => {
      let value = 2
      result = value + 20
    }
    _ => {}
  }

  return result
}

print ${pick(TagA)}:${pick(TagB)}:${pick(TagC)}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("match-relet-lowered.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(
        evaluator.lowered_pures.contains_key(&Name::intern("pick")),
        "statement match with sibling re-lets should lower"
    );

    let output = Evaluator::new_with_sources(Vec::new(), sources).eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"11:22:0\n");
    assert!(output.traceback.is_none());
}

#[test]
fn match_expr_bare_ident_fallback_lowers() {
    // A bare identifier as a match-expression arm value (`_ => fallback`) is parsed
    // as a tail-bare-ident statement, not an expression statement; the lowerer must
    // accept it. This is the shape the tokei `count_language` dispatcher needs.
    let source = r#"type Tag = TagA | TagB | TagC

pure pick(tag: Tag) -> Int {
  let fallback = -1

  match tag {
    TagA => 11
    TagB => 22
    _ => fallback
  }
}

print ${pick(TagA)}:${pick(TagB)}:${pick(TagC)}
"#;
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("match-bare-ident-lowered.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let checked = {
        let sid = parsed
            .arena
            .arena
            .span_source_id
            .expect("loaded arena should have a primary source");
        let text = sources
            .get(sid)
            .map(|s| s.text().to_string())
            .unwrap_or_default();
        Checker::check_arena(&parsed.arena, &text)
    };
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

    let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.install_compact_lowered_program(&parsed.arena, source_id);
    assert!(
        evaluator.lowered_pures.contains_key(&Name::intern("pick")),
        "match expression with a bare-ident fallback arm should lower"
    );

    let output = Evaluator::new_with_sources(Vec::new(), sources).eval(&parsed.arena, source_id);
    assert_eq!(output.stdout, b"11:22:-1\n");
    assert!(output.traceback.is_none());
}
