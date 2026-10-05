use super::common::*;

#[test]
fn xsht_lint_uses_nested_config_for_discovered_files() {
    let parent = temp_path("lint-nested-config-parent");
    let project = parent.join("project");
    let lib = project.join("lib");
    let app = project.join("app");
    let ignored = project.join("ignored");
    let _ = std::fs::remove_dir_all(&parent);
    std::fs::create_dir_all(&lib).expect("create lib dir");
    std::fs::create_dir_all(&app).expect("create app dir");
    std::fs::create_dir_all(&ignored).expect("create ignored dir");
    std::fs::write(
        project.join("xsht-config.ini"),
        "exclude = ignored/**\nmodule_path = lib\n",
    )
    .expect("write nested config");
    std::fs::write(
        lib.join("helper.xsh"),
        "##! Nested config helper module.\n## Returns the configured helper value.\nexport pure value() -> Str {\n  \"ok\"\n}\n",
    )
    .expect("write helper module");
    std::fs::write(app.join("main.xsh"), "use helper\nprint helper.value()\n")
        .expect("write app script");
    std::fs::write(ignored.join("bad.xsh"), "let =\n").expect("write ignored script");

    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsht"))
        .arg("lint")
        .current_dir(&parent)
        .output()
        .expect("run xsht");

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_eq!(stderr_text(&output), "");

    let _ = std::fs::remove_dir_all(parent);
}

#[test]
fn xsht_test_uses_current_directory_as_default_module_path() {
    let root = temp_path("xsht-default-module-path");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("tests")).expect("create native test dir");
    std::fs::write(
        root.join("helper.xsh"),
        r#"##! Default module path helper.
## Returns a value from the project root.
export pure value() -> Str {
  return "ok"
}
"#,
    )
    .expect("write helper module");
    std::fs::write(
        root.join("tests/main.xsh"),
        r#"use helper

test test_imported_helper [error] {
  test.eq(helper.value(), "ok")?
}
"#,
    )
    .expect("write native test");

    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsht"))
        .arg("test")
        .current_dir(&root)
        .output()
        .expect("run xsht");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("running 1 tests"));
    assert!(stdout.contains("tests/main.xsh::test_imported_helper ... ok"));
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(root);
}

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
