use super::common::*;

#[test]
fn ir_coverage_scans_multiline_top_level_regions_once() {
    let root = temp_path("ir-coverage-mini-root");
    let report = root.join("target/ir-coverage.json");
    let syntax = root.join("src/syntax");
    let runtime = root.join("src/runtime");
    let sema = root.join("src/sema");
    std::fs::create_dir_all(&syntax).expect("create syntax dir");
    std::fs::create_dir_all(&runtime).expect("create runtime dir");
    std::fs::create_dir_all(&sema).expect("create sema dir");
    std::fs::write(
        syntax.join("arena.rs"),
        r#"
pub enum ArenaStmtKind {
    Let,
    Var,
    Assign,
    If,
    While,
    For,
    Match,
    Return,
    Break,
    Continue,
    Command,
    Use,
}

pub enum ArenaExprKind {
    Bool,
    Int,
    Str,
    FmtString,
    Ident,
    Item,
    List,
    ListComp,
    StructuredPipeline,
    Record,
    Binary,
    Call,
    Field,
    Index,
    If,
    Match,
    Try,
    Run,
}

pub enum ArenaTypeExprTag {
    Named,
    List,
    Map,
    Result,
}
"#,
    )
    .expect("write arena source");
    std::fs::write(
        syntax.join("node.rs"),
        r#"

pub enum BinaryOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    In,
    NotIn,
    ResultFallback,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

pub enum AssignOp {
    Set,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}
"#,
    )
    .expect("write node source");
    std::fs::write(
        runtime.join("eval.rs"),
        r#"
enum LoweredPipelineStage {
    Map,
}

enum LoweredType {
    Str,
}

fn lowered_method_name(name: &str) -> bool {
    matches!(name, "lower" | "len")
}
"#,
    )
    .expect("write eval source");
    std::fs::create_dir_all(runtime.join("eval/indexed")).expect("create indexed runtime");
    std::fs::write(
        runtime.join("eval/indexed/full.rs"),
        r#"
enum FullTag {
    ExprStr,
    StmtLet,
}
"#,
    )
    .expect("write indexed source");
    std::fs::write(sema.join("records.rs"), "").expect("write records source");
    std::fs::write(
        root.join("script.xsh"),
        r#"
type Plugin = module {
  export proc execute(root: Path) [fs, error] -> Result[Unit, Error]
}

let records = """{"name":"small"}
{"name":"large"}
"""
  |> json.lines
  |> sort-by .name
print ${records[0].name}

let module_source = """\nexport proc execute(root: Path) [fs, error] -> Result[Unit, Error] {
  let status = {raw: true}
}
"""

let first = "alpha"
let names = [
  first,
]

pure helper(
  value: Str,
) -> Str {
  return value.lower()
}

pure render(fmt: Str) -> Str {
  if fmt == """%s
""" {
    return "line"
  }

  return fmt
}

let value = helper("OK")
"#,
    )
    .expect("write corpus script");

    let output = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"))
        .args([
            "tools/xsh-ir-coverage.xsh",
            "--",
            "--root",
            root.to_str().unwrap(),
            "--json",
            report.to_str().unwrap(),
        ])
        .output()
        .expect("run coverage tool");
    assert_ok(&output);

    let json = json_parse(&std::fs::read_to_string(&report).unwrap());
    let script = json_field(&json, "script");
    assert_eq!(json_u64(json_field(script, "total")), 6);
    assert_eq!(
        json_str(json_field(
            json_index(json_field(script, "reasons"), 0),
            "reason"
        )),
        "expr.pipeline"
    );
    assert!(
        json_array(json_field(script, "groups"))
            .iter()
            .any(|group| json_str(json_field(group, "group")) == "expression"
                && json_u64(json_field(group, "total")) == 1)
    );
    assert!(
        !json_array(json_field(script, "reasons"))
            .iter()
            .any(|reason| matches!(
                json_str(json_field(reason, "reason")),
                "stmt.TailBareIdent" | "stmt.Return"
            ))
    );
    assert!(
        !json_array(json_field(json_field(&json, "procs"), "reasons"))
            .iter()
            .any(|reason| json_str(json_field(reason, "reason")) == "type.param.true")
    );

    std::fs::remove_dir_all(root).expect("remove temp dir");
}

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
