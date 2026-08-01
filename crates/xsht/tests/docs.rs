use std::process::Command;
use std::sync::OnceLock;
use xsht::docs::{DocsReport, GeneratedFile};

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn docs_report() -> &'static DocsReport {
    static REPORT: OnceLock<DocsReport> = OnceLock::new();
    REPORT.get_or_init(|| xsht::docs::check(workspace_root()).expect("docs check"))
}

#[test]
fn xsht_docs_check_accepts_generated_docs() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["docs", "check"])
        .current_dir(workspace_root())
        .output()
        .expect("run xsht docs");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn docs_checker_accepts_current_workspace_directly() {
    docs_report();
}

#[test]
fn docs_generation_only_writes_code_derived_reference_outputs() {
    let report = docs_report();
    let paths = report
        .generated
        .iter()
        .map(|file| file.path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert_eq!(
        paths,
        vec!["docs/STDLIB.md".to_string(), "docs/REFERENCE.md".to_string()]
    );
}

#[test]
fn docs_generated_stdlib_contains_modules_methods_and_records() {
    let report = docs_report();
    let stdlib = generated_contents(report, "docs/STDLIB.md");

    assert!(stdlib.contains("### `fs`"));
    assert!(stdlib.contains("### `Str` Methods"));
    assert!(stdlib.contains("### `FsEntry`"));
}

fn generated_contents<'a>(report: &'a DocsReport, path: &str) -> &'a str {
    report
        .generated
        .iter()
        .find(|file| file.path == std::path::Path::new(path))
        .map(generated_file_contents)
        .unwrap_or_else(|| panic!("missing generated file {path}"))
}

fn generated_file_contents(file: &GeneratedFile) -> &str {
    &file.contents
}
