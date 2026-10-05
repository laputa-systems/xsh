use super::common::*;

#[test]
fn xsh_native_tests() {
    let entries = std::fs::read_dir("showcase").expect("read showcase dir");
    let mut scripts = Vec::new();

    for entry in entries {
        let entry = entry.expect("read showcase entry");
        let path = entry.path();
        if path.file_name().is_some_and(|name| name == "tests") {
            assert!(path.is_dir(), "showcase/tests must be a directory");
            continue;
        }
        if path.is_dir() {
            panic!("showcase subdirectories are no longer part of the layout: {path:?}");
        }
        if path.extension().is_some_and(|extension| extension == "md") {
            panic!("showcase READMEs moved into script header comments: {path:?}");
        }
        if path.extension().is_some_and(|extension| extension == "xsh") {
            scripts.push(path);
        }
    }

    scripts.sort();
    assert!(!scripts.is_empty());

    for script in &scripts {
        let name = script
            .file_stem()
            .expect("script has stem")
            .to_string_lossy();
        assert!(
            Path::new("showcase/tests")
                .join(format!("test-{name}.xsh"))
                .is_file(),
            "missing showcase test for {script:?}"
        );
    }

    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsht"))
        .args(["test"])
        .env("CARGO_BIN_EXE_xsh", cargo_env!("CARGO_BIN_EXE_xsh"))
        .env("CARGO_BIN_EXE_xsht", cargo_env!("CARGO_BIN_EXE_xsht"))
        .output()
        .expect("run showcase tests");

    assert!(
        output.status.success(),
        "xsh native tests\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
