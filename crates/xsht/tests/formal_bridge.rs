use std::path::Path;
use std::process::Command;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::arena::ArenaProgram;
use xsh::frontend::syntax::parser::Parser;

const FIXTURE_SOURCE: &str = include_str!(
    "../../../tests/fixtures/runtime/formal-bridge-scalar-literals.xsh"
);

fn function_binders(program: &ArenaProgram) -> Vec<String> {
    program
        .arena
        .function_defs
        .iter()
        .map(|function| function.name.as_str().to_string())
        .collect()
}

fn format_fixture_source(root: &Path, source: &str) -> String {
    let scratch = root.join(".work/tmp");
    std::fs::create_dir_all(&scratch).expect("create bridge formatter scratch directory");
    let directory = tempfile::Builder::new()
        .prefix("formal-bridge-")
        .tempdir_in(scratch)
        .expect("create bridge formatter fixture directory");
    let path = directory.path().join("fixture.xsh");
    std::fs::write(&path, source).expect("write bridge formatter fixture");
    let first = Command::new(test_bin!("xsht"))
        .arg("fmt")
        .arg(&path)
        .current_dir(root)
        .output()
        .expect("run formatter");
    assert_eq!(
        first.status.code(),
        Some(0),
        "formatter failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let formatted = std::fs::read_to_string(&path).expect("read formatted bridge fixture");
    let second = Command::new(test_bin!("xsht"))
        .arg("fmt")
        .arg(&path)
        .current_dir(root)
        .output()
        .expect("run formatter on its own output");
    assert_eq!(
        second.status.code(),
        Some(0),
        "formatter failed on its own output: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("read second formatted bridge fixture"),
        formatted
    );
    formatted
}

#[test]
fn formal_bridge_fixture_formats_idempotently_without_changing_function_binders() {
    let root = super::workspace_root();
    let original = Parser::parse_source_arena_only(SourceId::new(0), FIXTURE_SOURCE);
    assert!(original.diagnostics.is_empty(), "{:?}", original.diagnostics);
    let expected_binders = function_binders(&original.arena);
    let formatted_source = format_fixture_source(&root, FIXTURE_SOURCE);
    let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &formatted_source);
    assert!(reparsed.diagnostics.is_empty(), "{:?}", reparsed.diagnostics);
    assert_eq!(function_binders(&reparsed.arena), expected_binders);
}
