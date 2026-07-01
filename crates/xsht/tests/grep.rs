use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;
use xsht::cli::{grep_scripts, refactor_scripts};

fn temp_xsh(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("xsh-grep-test-{name}.xsh"));
    fs::write(&path, content).expect("write temp xsh file");
    path
}

fn paths(p: &Path) -> Vec<String> {
    vec![p.to_string_lossy().into_owned()]
}

fn output_text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf-8 output")
}

#[test]
fn grep_without_paths_uses_configured_includes() {
    let root = TempDir::new().expect("create temp root");
    let scripts = root.path().join(".github/scripts");
    fs::create_dir_all(&scripts).expect("create scripts dir");
    fs::write(
        root.path().join("xsht-config.ini"),
        "include = .github/scripts\n",
    )
    .expect("write config");
    fs::write(
        scripts.join("release.xsh"),
        "let items = [1]\nlet count = list.len(items)\n",
    )
    .expect("write release script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["grep", "list.len(EXPR)"])
        .current_dir(root.path())
        .output()
        .expect("run xsht grep");

    let stdout = output_text(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "stdout: {stdout}");
    assert!(
        stdout.contains(".github/scripts/release.xsh"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("list.len(items)"), "stdout: {stdout}");
}

#[test]
fn grep_basic_module_function_call() {
    let src = "let xs = [1, 2, 3]\nlet n = list.len(xs)\n";
    let file = temp_xsh("basic_module_call", src);
    let out = grep_scripts("list.len(EXPR)", &paths(&file));
    let stdout = output_text(&out.stdout);
    assert_eq!(out.status, 0, "stderr: {}", output_text(&out.stderr));
    assert!(
        stdout.contains("list.len(xs)"),
        "expected match in stdout: {}",
        stdout
    );
    assert!(stdout.contains("1 match"), "stdout: {}", stdout);
}

#[test]
fn grep_basic_no_match_exits_one() {
    let src = "let xs = [1, 2, 3]\nlet n = list.len(xs)\n";
    let file = temp_xsh("no_match_exit_one", src);
    // Pattern that won't match anything in this file
    let out = grep_scripts("list.len(EXPR)", &paths(&file));
    // We already tested a match above — now test a pattern with no matches
    let out2 = grep_scripts("map.get(M, K)", &paths(&file));
    let stdout2 = output_text(&out2.stdout);
    assert_eq!(out2.status, 1, "expected exit 1, stdout: {}", stdout2);
    assert!(stdout2.contains("0 matches"), "stdout: {}", stdout2);
    // The successful match from before must still be status 0
    assert_eq!(out.status, 0);
}

#[test]
fn grep_method_call() {
    let src = "let xs = []\nxs.push(v)\n";
    let file = temp_xsh("method_call", src);
    let out = grep_scripts("RECV.push(ITEM)", &paths(&file));
    let stdout = output_text(&out.stdout);
    assert_eq!(out.status, 0, "stderr: {}", output_text(&out.stderr));
    assert!(
        stdout.contains("xs.push(v)"),
        "expected match in stdout: {}",
        stdout
    );
}

#[test]
fn grep_inside_pipeline_block() {
    let src = concat!(
        "let items = []\n",
        "let result = items |> map { hash.sha256(item)? }\n",
    );
    let file = temp_xsh("pipeline_block", src);
    let out = grep_scripts("hash.sha256(P)", &paths(&file));
    let stdout = output_text(&out.stdout);
    assert_eq!(out.status, 0, "stderr: {}", output_text(&out.stderr));
    assert!(
        stdout.contains("hash.sha256(item)"),
        "expected match in stdout: {}",
        stdout
    );
}

#[test]
fn grep_inside_try() {
    // Pattern without ? should still match the inner call inside a Try node
    let src = "let h = hash.sha256(p)?\n";
    let file = temp_xsh("inside_try", src);
    let out = grep_scripts("hash.sha256(P)", &paths(&file));
    let stdout = output_text(&out.stdout);
    assert_eq!(out.status, 0, "stderr: {}", output_text(&out.stderr));
    assert!(
        stdout.contains("hash.sha256(p)"),
        "expected match in stdout: {}",
        stdout
    );
}

#[test]
fn grep_no_matches_returns_exit_one() {
    let src = "let x = 1\n";
    let file = temp_xsh("no_matches", src);
    let out = grep_scripts("list.len(EXPR)", &paths(&file));
    let stdout = output_text(&out.stdout);
    assert_eq!(out.status, 1, "stdout: {}", stdout);
    assert!(stdout.contains("0 matches"), "stdout: {}", stdout);
}

#[test]
fn grep_multiple_metavariables() {
    let src = "map.set(m, k, v)\n";
    let file = temp_xsh("multi_metavar", src);
    let out = grep_scripts("map.set(M, K, V)", &paths(&file));
    let stdout = output_text(&out.stdout);
    assert_eq!(out.status, 0, "stderr: {}", output_text(&out.stderr));
    assert!(
        stdout.contains("map.set(m, k, v)"),
        "expected match in stdout: {}",
        stdout
    );
    assert!(stdout.contains("1 match"), "stdout: {}", stdout);
}

#[test]
fn refactor_basic_rename() {
    let src = "let n = list.len(xs)\n";
    let file = temp_xsh("refactor_rename", src);
    let out = refactor_scripts("list.len(X)", "X.len()", &paths(&file), false);
    assert_eq!(out.status, 0, "stderr: {}", output_text(&out.stderr));
    // File should have been rewritten
    let new_src = fs::read_to_string(&file).expect("read file after refactor");
    assert!(
        new_src.contains("xs.len()"),
        "expected xs.len() in rewritten file: {new_src}"
    );
    assert!(
        !new_src.contains("list.len(xs)"),
        "old call should be gone: {new_src}"
    );
}

#[test]
fn refactor_dry_run_does_not_modify_file() {
    let src = "let n = list.len(xs)\n";
    let file = temp_xsh("refactor_dry_run", src);
    let out = refactor_scripts("list.len(X)", "X.len()", &paths(&file), true);
    let stdout = output_text(&out.stdout);
    assert_eq!(out.status, 0, "stderr: {}", output_text(&out.stderr));
    assert!(
        stdout.contains("dry run"),
        "expected dry run notice in stdout: {}",
        stdout
    );
    // File must be unchanged
    let after = fs::read_to_string(&file).expect("read file after dry run");
    assert_eq!(after, src, "file should be unchanged after dry run");
}

#[test]
fn refactor_no_op_when_no_matches() {
    let src = "let x = 1\n";
    let file = temp_xsh("refactor_noop", src);
    let out = refactor_scripts("list.len(X)", "X.len()", &paths(&file), false);
    // No matches → status 1, file unchanged
    assert_eq!(out.status, 1, "stdout: {}", output_text(&out.stdout));
    let after = fs::read_to_string(&file).expect("read file after no-op refactor");
    assert_eq!(
        after, src,
        "file should be unchanged when there are no matches"
    );
}
