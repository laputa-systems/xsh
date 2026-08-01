use std::io::Write;
use std::process::{Command, Stdio};

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn xsht(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(args)
        .current_dir(workspace_root())
        .output()
        .expect("run xsht")
}

#[test]
fn api_mixed_batch_preserves_query_order() {
    let output = xsht(&[
        "api",
        "api:json.read",
        "method:Path.read_text",
        "record:FsEntry",
        "language:run.status",
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("api: module.json.read"), "{stdout}");
    assert!(stdout.contains("api: method.Path.read_text"), "{stdout}");
    assert!(stdout.contains("api: record.FsEntry"), "{stdout}");
    assert!(stdout.contains("api: language.run.status"), "{stdout}");
    assert!(
        stdout.find("query: api:json.read") < stdout.find("query: method:Path.read_text")
            && stdout.find("query: method:Path.read_text") < stdout.find("query: record:FsEntry")
            && stdout.find("query: record:FsEntry") < stdout.find("query: language:run.status"),
        "{stdout}"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_jsonl_has_one_response_per_selector() {
    let output = xsht(&[
        "api",
        "--format",
        "jsonl",
        "api:json.read",
        "language:effect.process",
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].contains("\"schema_version\":1"), "{}", lines[0]);
    assert!(
        lines[0].contains("\"query\":\"api:json.read\""),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].contains("\"query\":\"language:effect.process\""),
        "{}",
        lines[1]
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_strict_renders_all_queries_before_failing() {
    let output = xsht(&["api", "--strict", "api:json.read", "api:json.missing"]);

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("query: api:json.read\nstatus: exact"),
        "{stdout}"
    );
    assert!(
        stdout.contains("query: api:json.missing\nstatus: missing"),
        "{stdout}"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn api_combines_query_file_and_argv_queries() {
    let root = tempfile::tempdir().expect("tempdir");
    let query_file = root.path().join("queries.txt");
    std::fs::write(&query_file, "api:json.read\nlanguage:effect.fs\n").expect("write query file");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args([
            "api",
            "--query-file",
            query_file.to_str().expect("query path"),
            "record:FsEntry",
        ])
        .current_dir(workspace_root())
        .output()
        .expect("run xsht");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.find("query: record:FsEntry") < stdout.find("query: api:json.read")
            && stdout.find("query: api:json.read") < stdout.find("query: language:effect.fs"),
        "{stdout}"
    );
}

#[test]
fn api_navigation_paths_resolve_in_the_workspace() {
    let root = workspace_root();
    for (id, docs) in xsh::modules::api_spec().docs_entries() {
        for path in docs
            .navigation
            .implementation
            .iter()
            .chain(&docs.navigation.tests)
        {
            let path = path
                .split_once("::")
                .map_or(path.as_str(), |(path, _)| path);
            assert!(root.join(path).is_file(), "{id}: {path}");
        }
        if let Some(showcase) = &docs.navigation.showcase {
            assert!(root.join(showcase).is_file(), "{id}: {showcase}");
        }
    }
    for name in xsh_registry::records::record_schemas().keys() {
        let docs = xsh_registry::records::record_docs(name);
        for path in docs
            .navigation
            .implementation
            .iter()
            .chain(&docs.navigation.tests)
        {
            assert!(root.join(path).is_file(), "record.{name}: {path}");
        }
    }
    for reference in xsh_registry::reference::language_references() {
        for path in reference
            .docs
            .navigation
            .implementation
            .iter()
            .chain(&reference.docs.navigation.tests)
        {
            assert!(
                root.join(path).is_file(),
                "language.{}: {path}",
                reference.id
            );
        }
    }
}

#[test]
fn api_stdin_queries_join_argv_batch_in_request_order() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["api", "record:FsEntry", "--stdin"])
        .current_dir(workspace_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start xsht");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"api:json.read\nlanguage:effect.fs\n")
        .expect("write queries");
    let output = child.wait_with_output().expect("wait xsht");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.find("query: record:FsEntry") < stdout.find("query: api:json.read")
            && stdout.find("query: api:json.read") < stdout.find("query: language:effect.fs"),
        "{stdout}"
    );
}

#[test]
fn api_search_is_local_and_deterministic() {
    let output = xsht(&["api", "search:rooted"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("status: matches"), "{stdout}");
    assert!(
        stdout.contains("api: module.archive.tar_extract"),
        "{stdout}"
    );
    assert!(stdout.contains("api: module.patch.apply"), "{stdout}");
}
