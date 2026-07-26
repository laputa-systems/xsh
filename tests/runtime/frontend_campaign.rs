use super::common::xsh;

#[test]
fn vertical_slice_indexed_execution_is_frozen() {
    let output = xsh(["tests/fixtures/frontend-campaign/vertical-slice.xsh"]);

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
fn formerly_unsupported_slice_runs_in_strict_indexed_mode() {
    let output = xsh([
        "--strict-lower",
        "tests/fixtures/frontend-campaign/vertical-slice-unsupported.xsh",
    ]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "non-empty\n");
    assert!(output.stderr.is_empty());
}
