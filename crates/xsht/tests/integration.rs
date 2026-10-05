#[macro_use]
#[path = "../../../tests/release_binary.rs"]
mod release_binary;

/// The stderr of `xsht check` or `xsht lint` without the stage timing line that
/// closes every run that processed a file. A run with no diagnostics leaves
/// nothing else.
fn stderr_before_timing_line<'a>(command: &str, stderr: &'a [u8]) -> &'a str {
    let text = std::str::from_utf8(stderr).expect("UTF-8 stderr");
    let body = text
        .strip_suffix('\n')
        .unwrap_or_else(|| panic!("xsht {command} stderr has no closing line: {text:?}"));
    let (diagnostics, timing) = match body.rfind('\n') {
        Some(end) => body.split_at(end + 1),
        None => ("", body),
    };
    assert!(
        timing.starts_with(&format!("xsht {command}: "))
            && timing.contains(" (thread time by stage: "),
        "xsht {command} stderr does not end with its timing line: {text:?}"
    );
    diagnostics
}

#[path = "api.rs"]
mod api;
#[path = "cli.rs"]
mod cli;
#[path = "desugar.rs"]
mod desugar;
#[path = "lint.rs"]
mod lint;
#[path = "lint_format_invariance.rs"]
mod lint_format_invariance;
#[path = "lint_performance.rs"]
mod lint_performance;
