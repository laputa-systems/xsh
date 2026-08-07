use std::io::Write;
use xsh::diagnostic::{Diagnostic, Severity};
use xsh::execution::evaluator::Evaluator;
use xsh::execution::script::{RunOptions, ScriptOutput, run_script};
use xsh::execution::value::Value;
use xsh::frontend::check::Checker;
use xsh::frontend::load::entry_source_from_text;
use xsh::frontend::source::{SourceId, SourceMap, Span};
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::parser::Parser;
use xsh::host::json::parse_raw_json;
use xsh::process::ProcessStatus;
use xsh::trace::model::{TraceEvent, TraceKind};

#[test]
fn facade_exposes_frontend_execution_process_and_trace_contracts() {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("<facade>", "proc main() { return }");
    let parsed = Parser::parse_source_arena_only(source_id, "proc main() { return }");
    assert!(parsed.diagnostics.is_empty());

    let checked = Checker::check_arena(&parsed.arena, "proc main() { return }");
    assert!(checked.diagnostics.is_empty());

    let span = Span::at(source_id, 0);
    let diagnostic = Diagnostic::warning("facade smoke test").with_span(span);
    assert_eq!(diagnostic.severity, Severity::Warning);

    let options = RunOptions {
        script: "script.xsh".to_string(),
        args: Vec::new(),
        coverage_trace_dir: None,
    };
    assert_eq!(options.args, Vec::<String>::new());

    let output = ScriptOutput {
        status: 0,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    assert_eq!(output.status, 0);
    assert_eq!(Value::Unit.type_name(), "Unit");
    let _evaluator = Evaluator::new(Vec::new());
    assert!(
        entry_source_from_text("<facade>", "".to_string())
            .diagnostics
            .is_empty()
    );
    assert_eq!(Name::INT.as_str().as_str(), "Int");
    assert!(parse_raw_json("{}").is_ok());
    assert_eq!(
        ProcessStatus::exited(0).kind,
        xsh::process::ProcessStatusKind::Exit
    );

    let trace = TraceEvent::new(1, TraceKind::ScriptEnter);
    assert_eq!(trace.event_id, 1);
}

#[test]
fn facade_script_execution_preserves_script_output_contract() {
    let mut script = tempfile::Builder::new()
        .suffix(".xsh")
        .tempfile()
        .expect("temporary script");
    writeln!(script, "print \"facade\"").expect("write temporary script");

    let output = run_script(RunOptions {
        script: script.path().display().to_string(),
        args: Vec::new(),
        coverage_trace_dir: None,
    });
    assert_eq!(output.status, 0);
    assert_eq!(output.stdout, b"facade\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn facade_keeps_source_identity_explicit() {
    let source_id = SourceId::new(7);
    let span = Span::new(source_id, 2, 5);
    assert_eq!(span.source_id, source_id);
    assert_eq!(span.start(), 2);
    assert_eq!(span.end(), 5);
}
