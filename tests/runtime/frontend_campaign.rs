use super::common::xsh;

#[test]
fn vertical_slice_arena_oracle_is_frozen() {
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
fn unsupported_slice_remains_an_honest_strict_lower_blocker() {
    let output = xsh([
        "--strict-lower",
        "tests/fixtures/frontend-campaign/vertical-slice-unsupported.xsh",
    ]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("compact.unlowered-main"), "stderr: {stderr}");
    assert!(stderr.contains("sources.len"), "stderr: {stderr}");
    assert!(output.stdout.is_empty());
}
