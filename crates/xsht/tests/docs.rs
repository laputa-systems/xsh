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
fn docs_checker_covers_static_html_outputs() {
    let report = docs_report();
    let paths = report
        .generated
        .iter()
        .map(|file| file.path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert!(paths.contains(&"docs-html/index.html".to_string()));
    assert!(paths.contains(&"docs-html/style.css".to_string()));
    assert!(paths.contains(&"docs-html/CHAPTER-01-why-xsh.html".to_string()));
    assert!(paths.contains(&"docs-html/CHAPTER-02-foundations.html".to_string()));
    assert!(paths.contains(&"docs-html/CHAPTER-03-tooling.html".to_string()));
    assert!(paths.contains(&"docs-html/CHAPTER-13-tracing.html".to_string()));
    assert!(paths.contains(&"docs-html/CHAPTER-15-why-not-xsh.html".to_string()));
    assert!(paths.contains(&"docs-html/STDLIB.html".to_string()));
    assert!(paths.contains(&"docs-html/stdlib/index.html".to_string()));
    assert!(paths.contains(&"docs-html/stdlib/module/fs.html".to_string()));
    assert!(paths.contains(&"docs-html/stdlib/methods/Str.html".to_string()));
    assert!(paths.contains(&"docs-html/stdlib/record/FsEntry.html".to_string()));
    assert!(paths.contains(&"docs-html/REFERENCE.html".to_string()));
}

#[test]
fn docs_generation_excludes_removed_showcase_catalog() {
    let report = docs_report();
    let paths = report
        .generated
        .iter()
        .map(|file| file.path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert!(!paths.contains(&"docs/SHOWCASE.md".to_string()));
    assert!(!paths.contains(&"docs-html/SHOWCASE.html".to_string()));
}

#[test]
fn docs_generated_html_highlights_xsh_fences() {
    let report = docs_report();
    let chapter = generated_contents(report, "docs-html/CHAPTER-08-structured-streams.html");

    assert!(chapter.contains("<span class=\"tok-keyword\">let</span>"));
    assert!(chapter.contains("<span class=\"tok-operator\">|&gt;</span>"));
}

#[test]
fn docs_generated_html_has_page_navigation() {
    let report = docs_report();
    let chapter = generated_contents(report, "docs-html/CHAPTER-08-structured-streams.html");

    assert!(chapter.contains("class=\"page-toc\""));
    assert!(chapter.contains("href=\"#the-pipeline-model\""));
    assert!(chapter.contains("rel=\"prev\""));
    assert!(chapter.contains("rel=\"next\""));
}

#[test]
fn docs_xsh_reference_fences_are_not_xsh_highlighted() {
    let report = docs_report();
    let reference = generated_contents(report, "docs-html/REFERENCE.html");

    assert!(reference.contains("language-xsh-reference"));
    assert!(!reference.contains("tok-keyword"));
    assert!(!reference.contains("tok-operator"));
}

#[test]
fn docs_generated_stdlib_contains_modules_methods_and_records() {
    let report = docs_report();
    let stdlib = generated_contents(report, "docs/STDLIB.md");

    assert!(stdlib.contains("### `fs`"));
    assert!(stdlib.contains("### `Str` Methods"));
    assert!(stdlib.contains("### `FsEntry`"));
}

#[test]
fn docs_highlighted_xsh_html_escapes_source_text() {
    let report = docs_report();
    let chapter = generated_contents(report, "docs-html/CHAPTER-02-foundations.html");

    assert!(chapter.contains("<span class=\"tok-operator\">&lt;</span>"));
    assert!(!chapter.contains("tries < 4"));
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
