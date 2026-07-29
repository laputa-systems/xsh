use super::common::xsh;

#[test]
fn indexed_execution_fixture_runs_on_the_standard_path() {
    let output = xsh(["tests/fixtures/frontend-indexed/indexed-execution.xsh"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "slice 13 120 true true\n"
    );
}

#[test]
fn indexed_method_call_fixture_runs_on_the_standard_path() {
    let output = xsh(["tests/fixtures/frontend-indexed/indexed-method-call.xsh"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "non-empty\n");
    assert!(output.stderr.is_empty());
}
