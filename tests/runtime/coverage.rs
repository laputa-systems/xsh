use super::common::*;

#[test]
fn reassigning_let_is_check_error() {
    let path = write_temp_script("reassign-let-check-error", "let x = 1\nx = 2\n");
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg(path.to_str().unwrap())
        .output()
        .expect("run xsh");

    assert_exit(&output, 2);
    assert_stderr_contains(&output, "check.assign-let");
    // The diagnostic should teach the discoverable mutable-binding token.
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("declare with `var`"),
        "expected assign-let diagnostic to name `var`: {stderr}"
    );

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsh_refuses_checker_errors_before_execution() {
    let path = write_temp_script(
        "checker-gate-before-execution",
        "print \"before\"\nlet value = \"abc\"\nprint $value.length()\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg(&path)
        .output()
        .expect("run xsh");

    assert_exit(&output, 2);
    assert!(
        output.stdout.is_empty(),
        "checker errors must prevent execution"
    );
    assert_stderr_contains(&output, "check.unknown-method");

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn runtime_unknown_method_names_receiver_and_candidates() {
    let path = write_temp_script(
        "runtime-unknown-method-context",
        "let value: Any = \"abc\"\nprint $value.length()\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg(&path)
        .output()
        .expect("run xsh");

    assert_exit(&output, 3);
    assert_stderr_contains(&output, "unknown method `length` on Str");
    assert_stderr_contains(&output, "count_chars");

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsht_check_uses_shared_pipeline() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "tests/fixtures/runtime/cli-simple.xsh"])
        .output()
        .expect("run xsht");

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn xsht_check_defaults_to_current_directory_and_respects_excludes() {
    let dir = temp_path("check-default-dir");
    std::fs::create_dir_all(dir.join("ignored")).expect("create temp dir");
    std::fs::write(dir.join("ok.xsh"), "let value = 1\n").expect("write ok script");
    std::fs::write(dir.join("ignored").join("bad.xsh"), "let =\n").expect("write bad script");
    std::fs::write(dir.join("xsht-config.ini"), "exclude = ignored/**\n").expect("write config");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .arg("check")
        .current_dir(&dir)
        .output()
        .expect("run xsht");

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_eq!(stderr_text(&output), "");

    std::fs::remove_dir_all(dir).expect("remove temp dir");
}

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
  export proc execute(root: Path) [fs, error] -> Result[Unit]
}

let records = """{"name":"small"}
{"name":"large"}
"""
  |> json.lines
  |> sort-by .name
print ${records[0].name}

let module_source = """
export proc execute(root: Path) [fs, error] -> Result[Unit] {
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

    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
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
fn xsht_check_accepts_directories_and_reports_failures() {
    let dir = temp_path("check-explicit-dir");
    std::fs::create_dir_all(dir.join("scripts")).expect("create temp dir");
    std::fs::write(dir.join("scripts").join("ok.xsh"), "let value = 1\n").expect("write ok script");
    std::fs::write(dir.join("scripts").join("bad.xsh"), "let =\n").expect("write bad script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "scripts"])
        .current_dir(&dir)
        .output()
        .expect("run xsht");

    assert_exit(&output, 2);
    assert_eq!(stdout_text(&output), "");
    assert_stderr_contains(&output, "parse.expected-ident");

    std::fs::remove_dir_all(dir).expect("remove temp dir");
}

#[test]
fn xsht_check_strict_fails_on_strict_warnings_only() {
    let path = temp_xsh_path("check-strict-any");
    std::fs::write(
        &path,
        r#"
type Row = {name: Str}
let row: Row = json.decode("{\"name\":\"demo\"}")?
"#,
    )
    .expect("write temp script");

    let strict = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "--strict", path.to_str().unwrap()])
        .output()
        .expect("run xsht strict");
    assert_exit(&strict, 2);
    assert_stderr_contains(&strict, "warn[check.strict-any]");

    let normal = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("run xsht check");
    assert_ok(&normal);

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsht_check_annotate_rewrites_safe_annotations() {
    let path = temp_xsh_path("check-annotate");
    std::fs::write(
        &path,
        r#"
let count=1
var names=["a", "b"]
let _ = process.command_argv("echo", ["ok"])
let data: Any = json.decode("{}")?
let row = {name: "demo"}
export let label="demo"
proc local(input = Path(".")) {}
export proc entry(flag = true) {}
"#,
    )
    .expect("write temp script");

    let output = xsht(["check", "--annotate", path.to_str().unwrap()]);

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_eq!(stderr_text(&output), "");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        r#"let count = 1
var names = ["a", "b"]
let _ = process.command_argv("echo", ["ok"])
let data: Any = json.decode("{}")?
let row = {name: "demo"}

export let label: Str = "demo"

proc local(input: Path = Path(".")) {}

export proc entry(flag: Bool = true) -> Result[Unit] {}
"#
    );

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsht_check_annotate_locals_rewrites_local_shapes() {
    let path = temp_xsh_path("check-annotate-locals");
    std::fs::write(
        &path,
        r#"
let count = 1
let names = ["a", "b"]
let command = process.command_argv("echo", names)
"#,
    )
    .expect("write temp script");

    let output = xsht(["check", "--annotate=locals", path.to_str().unwrap()]);

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_eq!(stderr_text(&output), "");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        r#"let count = 1
let names: List[Str] = ["a", "b"]
let command: Command = process.command_argv("echo", names)
"#
    );

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsht_check_annotate_uses_exact_configured_classes() {
    let dir = temp_path("check-annotate-config");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    std::fs::write(
        dir.join("xsht-config.ini"),
        "[check]\nannotate = locals\n  exports\n",
    )
    .expect("write config");
    let path = dir.join("main.xsh");
    std::fs::write(
        &path,
        r#"
let names = ["a", "b"]
export let label = "demo"
proc local(input = Path(".")) {}
"#,
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "--annotate", "main.xsh"])
        .current_dir(&dir)
        .output()
        .expect("run xsht");

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_eq!(stderr_text(&output), "");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        r#"let names: List[Str] = ["a", "b"]

export let label: Str = "demo"

proc local(input = Path(".")) {}
"#
    );

    std::fs::remove_dir_all(dir).expect("remove temp dir");
}

#[test]
fn xsht_check_annotate_skips_unsafe_or_unhelpful_types() {
    let path = temp_xsh_path("check-annotate-skips");
    std::fs::write(
        &path,
        r#"let {name} = {name: "demo"}
let data = json.decode("{}")?
let row = {name: "demo"}
let unit = abort(0)
"#,
    )
    .expect("write temp script");

    let output = xsht(["check", "--annotate=locals", path.to_str().unwrap()]);

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_eq!(stderr_text(&output), "");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        r#"let {name} = {name: "demo"}
let data = json.decode("{}")?
let row = {name: "demo"}
let unit = abort(0)
"#
    );

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsht_check_annotate_rewrites_only_requested_script() {
    let dir = temp_path("check-annotate-import");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let helper = dir.join("helper.xsh");
    let main = dir.join("main.xsh");
    let helper_source =
        "##! Annotate helper module.\n## Exposes the imported value.\nexport let value = 1\n";
    std::fs::write(&helper, helper_source).expect("write helper");
    std::fs::write(
        &main,
        "use helper\nproc local(input = Path(\".\")) {}\nlet names = [\"a\"]\n",
    )
    .expect("write main");

    let output = xsht(["check", "--annotate", main.to_str().unwrap()]);

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_eq!(stderr_text(&output), "");
    assert_eq!(std::fs::read_to_string(&helper).unwrap(), helper_source);
    assert_eq!(
        std::fs::read_to_string(&main).unwrap(),
        "use helper\n\nproc local(input: Path = Path(\".\")) {}\n\nlet names = [\"a\"]\n"
    );

    std::fs::remove_dir_all(dir).expect("remove temp dir");
}

#[test]
fn xsht_check_annotate_does_not_write_on_strict_diagnostics() {
    let path = temp_xsh_path("check-annotate-strict");
    let source = r#"
type Row = {name: Str}
let row: Row = json.decode("{\"name\":\"demo\"}")?
"#;
    std::fs::write(&path, source).expect("write temp script");

    let output = xsht(["check", "--strict", "--annotate", path.to_str().unwrap()]);

    assert_exit(&output, 2);
    assert_stderr_contains(&output, "warn[check.strict-any]");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsht_check_reveals_type_without_failing() {
    let path = write_temp_script(
        "check-reveal-type",
        r#"
let names = ["a", "b"]
reveal_type(names)
"#,
    );

    let output = xsht(["check", path.to_str().unwrap()]);

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_stderr_contains(&output, "note[check.reveal-type]: revealed type: List[Str]");

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsh_rejects_reveal_type() {
    let path = write_temp_script("run-reveal-type", "reveal_type(1)\n");

    let output = xsh([path.to_str().unwrap()]);

    assert_exit(&output, 2);
    assert_stderr_contains(&output, "err[check.reveal-type]");

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsht_fmt_check_accepts_stable_examples() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args([
            "fmt",
            "--check",
            "tests/fixtures/runtime/cli-simple.xsh",
            "tests/fixtures/runtime/cli-args.xsh",
        ])
        .output()
        .expect("run xsht");

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_eq!(stderr_text(&output), "");
}

#[test]
fn runnable_xsh_corpus_is_formatted_and_lints_without_warnings() {
    let tracked = Command::new("git")
        .args(["ls-files", "*.xsh"])
        .output()
        .expect("list tracked XSH files");
    assert_ok(&tracked);
    let tracked_files = stdout_text(&tracked);
    let files = tracked_files
        .lines()
        .filter(|path| {
            // Fixtures and API snippets are intentionally non-runnable source:
            // the former exercise parser/runtime edge cases, while the latter
            // may use illustrative placeholders.
            !path.starts_with("tests/fixtures/") && !path.starts_with("docs/snippets/")
        })
        .collect::<Vec<_>>();

    let formatted = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["fmt", "--check"])
        .args(&files)
        .output()
        .expect("run xsht fmt");
    assert_ok(&formatted);
    assert_eq!(stdout_text(&formatted), "");
    assert_eq!(stderr_text(&formatted), "");

    let linted = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .arg("lint")
        .args(files)
        .output()
        .expect("run xsht lint");
    assert_ok(&linted);
    assert_eq!(stdout_text(&linted), "");
    assert_eq!(stderr_text(&linted), "");
}

#[test]
fn xsht_fmt_writes_canonical_source() {
    let path = temp_xsh_path("fmt-writes");
    std::fs::write(
        &path,
        "proc main(args:List[Str])->Result[Unit]{return Ok()}\n",
    )
    .expect("write temp script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["fmt", path.to_str().unwrap()])
        .output()
        .expect("run xsht");

    assert_ok(&output);
    assert_eq!(stdout_text(&output), "");
    assert_eq!(stderr_text(&output), "");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "proc main(args: List[Str]) -> Result[Unit] {\n  return Ok()\n}\n"
    );

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsht_fmt_check_reports_unformatted_files() {
    let path = temp_xsh_path("fmt-check");
    std::fs::write(&path, "let x=1\n").expect("write temp script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["fmt", "--check", path.to_str().unwrap()])
        .output()
        .expect("run xsht");

    assert_exit(&output, 1);
    assert!(stdout_text(&output).contains("needs formatting"));
    assert_eq!(stderr_text(&output), "");

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsht_fmt_check_reports_discovered_files_in_stable_order() {
    let dir = temp_path("fmt-check-order");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    std::fs::write(dir.join("b.xsh"), "let b=1\n").expect("write b script");
    std::fs::write(dir.join("a.xsh"), "let a=1\n").expect("write a script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["fmt", "--check"])
        .current_dir(&dir)
        .output()
        .expect("run xsht");

    assert_exit(&output, 1);
    let stdout = stdout_text(&output);
    assert!(
        stdout.find("a.xsh: needs formatting").unwrap()
            < stdout.find("b.xsh: needs formatting").unwrap()
    );
    assert_eq!(stderr_text(&output), "");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn xsht_lint_reports_warnings_with_spans() {
    let path = temp_xsh_path("lint-warnings");
    let source = "\
proc main(args: List[Str]) {
  let input = args[0]
  let src = \"tmp\"
  let root = Path(\"target/lint\")
  let unused = 1
  let p = Path(src)
  fs.mkdir(fp\"${root}/src/lib\", parents: true)?
  run grep (input) haystack ?
  if true {
    let src = \"other\"
    print ${src} ${args[0]}
  }
}

main(args)?
";
    std::fs::write(&path, source).expect("write temp script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["lint", path.to_str().unwrap()])
        .output()
        .expect("run xsht");

    assert_exit(&output, 1);
    assert_eq!(stdout_text(&output), "");
    assert_stderr_contains(&output, "warn[lint.unused-local]");
    assert_stderr_contains(&output, "warn[lint.path-constructor]");
    assert_stderr_contains(&output, "warn[lint.command-value]");
    assert_stderr_contains(&output, "warn[lint.redundant-default]");
    assert_stderr_contains(&output, path.to_str().unwrap());

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsht_lint_reports_discovered_files_in_stable_order() {
    let dir = temp_path("lint-order");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let source = "\
proc main() {
  let unused = 1
}

main()?
";
    std::fs::write(dir.join("b.xsh"), source).expect("write b script");
    std::fs::write(dir.join("a.xsh"), source).expect("write a script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .arg("lint")
        .current_dir(&dir)
        .output()
        .expect("run xsht");

    assert_exit(&output, 1);
    let stderr = stderr_text(&output);
    assert!(stderr.find("a.xsh").unwrap() < stderr.find("b.xsh").unwrap());
    assert_eq!(stdout_text(&output), "");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn xsht_lint_mixed_parse_and_lint_failures_exit_with_parse_status() {
    let dir = temp_path("lint-mixed-status");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    std::fs::write(dir.join("a.xsh"), "let =\n").expect("write bad script");
    std::fs::write(
        dir.join("b.xsh"),
        "\
proc main() {
  let unused = 1
}

main()?
",
    )
    .expect("write warning script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .arg("lint")
        .current_dir(&dir)
        .output()
        .expect("run xsht");

    assert_exit(&output, 2);
    assert_stderr_contains(&output, "err[parse.");
    assert_stderr_contains(&output, "warn[lint.unused-local]");

    let _ = std::fs::remove_dir_all(dir);
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

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
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
fn xsht_check_rejects_undefined_utility_commands() {
    let path = temp_xsh_path("check-interactive-command");
    std::fs::write(&path, "echo hi\n").expect("write temp script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("run xsht");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("err[check.unresolved-proc-command]"));
    assert!(stderr.contains("unresolved proc command"));

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsht_check_ignores_xshi_config_aliases() {
    let home = temp_path("check-ignores-xshi-config");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".config/xshi")).expect("create config dir");
    std::fs::write(
        home.join(".config/xshi/config.xsh"),
        r#"{
  aliases: [
    {name: "echo", source: "print"},
  ],
}
"#,
    )
    .expect("write config");

    let path = temp_xsh_path("check-ignores-interactive-config");
    std::fs::write(&path, "echo hi\n").expect("write temp script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .env("HOME", &home)
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("run xsht");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("err[check.unresolved-proc-command]"));
    assert!(!home.join(".local/share/xshi/history").exists());

    std::fs::remove_file(path).expect("remove temp script");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn xsht_lint_reports_check_errors_with_spans() {
    let path = write_temp_script("lint-reassign-let", "let x = 1\nx = 2\n");
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["lint", path.to_str().unwrap()])
        .output()
        .expect("run xsht");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("err[check.assign-let]"));
    assert!(stderr.contains(path.to_str().unwrap()));

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsht_lint_reports_imported_check_errors_once() {
    let root = temp_path("lint-import-dedupe");
    let lib = root.join("lib");
    std::fs::create_dir_all(&lib).expect("create lint temp dir");
    let shared = lib.join("shared.xsh");
    let first = root.join("first.xsh");
    let second = root.join("second.xsh");
    std::fs::write(
        &shared,
        "export pure bad() -> Int {\n  return \"not an int\"\n}\n",
    )
    .expect("write shared module");
    std::fs::write(&first, "use lib.shared\nlet one = shared.bad()\n").expect("write first");
    std::fs::write(&second, "use lib.shared\nlet two = shared.bad()\n").expect("write second");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["lint", first.to_str().unwrap(), second.to_str().unwrap()])
        .output()
        .expect("run xsht");
    let _ = std::fs::remove_dir_all(root);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.matches("err[check.type-mismatch]").count(), 1);
    assert!(stderr.contains(shared.to_str().unwrap()));
}

#[test]
fn xsht_lint_accepts_current_syntax_and_ignores_strings_and_comments() {
    let path = temp_xsh_path("stale-current");
    std::fs::write(
        &path,
        "\
# old examples: fmt\"x\" glob\"*.rs\" run.capture --text echo $name run (target)
let label = f\"hello\"
let files = g\"*.rs\"
let shell = \"printf '$HOME' run.capture (target)\"
let target = p\"target/debug/tool\"
let opts = {tool: target}
run.status $target --flag $opts.tool
print ${label}
",
    )
    .expect("write temp script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["lint", path.to_str().unwrap()])
        .output()
        .expect("run xsht");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    std::fs::remove_file(path).expect("remove temp script");
}

#[test]
fn xsht_ast_prints_parser_debug_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["ast", "tests/fixtures/runtime/cli-trace.xsh"])
        .output()
        .expect("run xsht");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Program"));
    assert!(stdout.contains("ProcDef"));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.is_empty());
}

#[test]
fn xsht_test_lists_and_filters_native_tests() {
    let listed = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["test", "--list", "test_dns"])
        .output()
        .expect("run xsht");
    let exact = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["test", "--exact", "tests/xsh/basic.xsh::test_pass"])
        .output()
        .expect("run xsht");

    assert!(listed.status.success());
    assert_eq!(
        String::from_utf8(listed.stdout).unwrap(),
        "tests/xsh/basic.xsh::test_dns_mock\ntests/xsh/stdlib/dns.xsh::test_dns_module_with_mocks\n"
    );
    assert!(exact.status.success());
    let stdout = String::from_utf8(exact.stdout).unwrap();
    assert!(stdout.contains("running 1 tests"));
    assert!(stdout.contains("tests/xsh/basic.xsh::test_pass ... ok"));
}

#[test]
fn xsht_test_discovers_tests_from_current_directory() {
    let root = temp_path("xsht-cwd-tests");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("tests/sub")).expect("create native test dir");
    std::fs::write(
        root.join("tests/sub/main.xsh"),
        r#"
proc test_alpha() [error] {
  test.eq("a", "a")?
}

proc test_beta() [error] {
  test.eq("b", "b")?
}
"#,
    )
    .expect("write native test");

    let listed = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["test", "--list"])
        .current_dir(&root)
        .output()
        .expect("run xsht");
    let filtered = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["test", "beta"])
        .current_dir(&root)
        .output()
        .expect("run xsht");

    assert!(listed.status.success());
    assert_eq!(
        String::from_utf8(listed.stdout).unwrap(),
        "tests/sub/main.xsh::test_alpha\ntests/sub/main.xsh::test_beta\n"
    );
    assert!(filtered.status.success());
    let stdout = String::from_utf8(filtered.stdout).unwrap();
    assert!(stdout.contains("running 1 tests"));
    assert!(stdout.contains("tests/sub/main.xsh::test_beta ... ok"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn xsht_test_succeeds_when_current_directory_has_no_tests_dir() {
    let root = temp_path("xsht-no-tests");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create empty test root");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .arg("test")
        .current_dir(&root)
        .output()
        .expect("run xsht");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 skipped\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn xsht_test_uses_cwd_config_for_excludes_and_module_path() {
    let root = temp_path("xsht-cwd-config");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("tests/ignored")).expect("create ignored tests");
    std::fs::create_dir_all(root.join("lib")).expect("create lib");
    std::fs::write(
        root.join("xsht-config.ini"),
        "exclude = tests/ignored/**/*.xsh\nmodule_path = lib\n",
    )
    .expect("write config");
    std::fs::write(
        root.join("lib/helper.xsh"),
        r#"##! CWD config helper module.
## Returns the helper value for the native test.
export pure value() -> Str {
  return "ok"
}
"#,
    )
    .expect("write helper module");
    std::fs::write(
        root.join("tests/main.xsh"),
        r#"use helper

proc test_imported_helper() [error] {
  test.eq(helper.value(), "ok")?
}
"#,
    )
    .expect("write native test");
    std::fs::write(
        root.join("tests/ignored/bad.xsh"),
        r#"print "this excluded file is not a native test"
"#,
    )
    .expect("write ignored test");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
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
fn xsht_test_reports_failures_and_can_keep_temp_roots() {
    let root = temp_path("xsht-test-failure-workflow");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("tests")).expect("create native test dir");
    std::fs::write(
        root.join("tests/main.xsh"),
        r#"
proc test_alpha(ctx: TestContext) [fs, io, error] {
  print ${ctx.temp_root.display()}
  print "alpha stdout"
  eprint "alpha stderr"
  fp"${ctx.temp_root}/marker".write("kept")?
  test.fail("alpha failed")?
}

proc test_beta() [error] {
  test.fail("beta should not run")?
}
"#,
    )
    .expect("write failing native tests");

    let fail_fast = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["test", "--keep-temp", "--fail-fast"])
        .current_dir(&root)
        .output()
        .expect("run xsht fail-fast");

    assert_exit(&fail_fast, 1);
    let stdout = stdout_text(&fail_fast);
    assert!(stdout.contains("running 2 tests"), "{stdout}");
    assert!(
        stdout.contains("tests/main.xsh::test_alpha ... FAILED"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("test tests/main.xsh::test_beta"),
        "{stdout}"
    );
    assert!(stdout.contains("stdout:\n"), "{stdout}");
    assert!(stdout.contains("alpha stdout"), "{stdout}");
    assert!(stdout.contains("stderr:\nalpha stderr"), "{stdout}");
    assert!(stdout.contains("test-fail: alpha failed"), "{stdout}");
    let temp_root_line = stdout
        .lines()
        .find(|line| line.contains("/xsh-test-") && line.ends_with("-test_alpha"))
        .expect("captured temp root path");
    let temp_root = PathBuf::from(temp_root_line);
    assert_eq!(
        std::fs::read_to_string(temp_root.join("marker")).expect("read retained marker"),
        "kept"
    );
    std::fs::remove_dir_all(&temp_root).expect("remove retained temp root");

    let nocapture = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args([
            "test",
            "--nocapture",
            "--exact",
            "tests/main.xsh::test_alpha",
        ])
        .current_dir(&root)
        .output()
        .expect("run xsht nocapture");

    assert_exit(&nocapture, 1);
    let stdout = stdout_text(&nocapture);
    assert!(stdout.contains("alpha stdout"), "{stdout}");
    assert!(!stdout.contains("stdout:\n"), "{stdout}");
    assert_eq!(stderr_text(&nocapture), "alpha stderr\n");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn xsht_test_runs_catalog_examples_only_when_requested() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["test", "--examples", "--exact", "examples::release-package"])
        .output()
        .expect("run xsht");
    let all = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["test", "--all", "--list", "release-package"])
        .output()
        .expect("run xsht");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("running 1 tests"));
    assert!(stdout.contains("examples::release-package ... ok"));
    assert!(all.status.success());
    assert_eq!(
        String::from_utf8(all.stdout).unwrap(),
        "examples::release-package\n"
    );
}

#[test]
fn xsht_test_cov_list_does_not_execute_tests() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args([
            "test",
            "--cov",
            "--list",
            "--exact",
            "tests/xsh/basic.xsh::test_pass",
        ])
        .output()
        .expect("run xsht");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout, "tests/xsh/basic.xsh::test_pass\n");
    assert!(!stdout.contains("coverage report"));
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn xsht_test_cov_exact_prints_coverage_sections() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["test", "--cov", "--exact", "tests/xsh/basic.xsh::test_pass"])
        .output()
        .expect("run xsht");

    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("running 1 tests"));
    assert!(stdout.contains("tests/xsh/basic.xsh::test_pass ... ok"));
    assert!(stdout.contains("coverage report"));
    assert!(stdout.contains("API coverage"));
    assert!(stdout.contains("uncovered standard APIs"));
}

#[test]
fn xsht_test_cov_json_out_writes_structured_report() {
    let path = temp_path("xsht-cov-json").with_extension("json");
    let _ = std::fs::remove_file(&path);

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args([
            "test",
            "--exact",
            "tests/xsh/basic.xsh::test_pass",
            "--cov-json",
            path.to_str().unwrap(),
        ])
        .output()
        .expect("run xsht");

    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("running 1 tests"));
    assert!(!stdout.contains("coverage report"));

    let json = json_parse(&std::fs::read_to_string(&path).unwrap());
    assert!(matches!(
        json_field(&json, "api_hits"),
        JsonValue::Object(_)
    ));
    assert!(
        json_array(json_field(&json, "standard_apis"))
            .iter()
            .any(|value| json_str(value) == "module.test.eq")
    );
    assert!(
        json_u64(json_field(
            json_field(json_field(&json, "api_hits"), "module.test.eq"),
            "tests"
        )) > 0
    );
}

#[test]
fn xsht_test_cov_json_includes_nested_xsh_processes() {
    let root = temp_path("xsht-nested-cov");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("tests")).expect("create native test dir");
    let child = root.join("child.xsh");
    let report = root.join("coverage.json");
    std::fs::write(&child, "print ${cpu.count()}\n").expect("write child script");
    std::fs::write(
        root.join("tests/main.xsh"),
        format!(
            r#"
proc test_child_coverage() [process, error] {{
  let output = run.text (Path({})) (Path({})) ?
  test.ok(output.trim().parse_int()? > 0)?
}}
"#,
            xsh_string_literal(env!("CARGO_BIN_EXE_xsh")),
            xsh_string_literal(child.to_str().unwrap()),
        ),
    )
    .expect("write native test");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args([
            "test",
            "--exact",
            "tests/main.xsh::test_child_coverage",
            "--cov-json",
            report.to_str().unwrap(),
        ])
        .current_dir(&root)
        .output()
        .expect("run xsht");

    assert!(output.status.success(), "{:?}", output);
    let json = json_parse(&std::fs::read_to_string(&report).unwrap());
    assert!(
        json_u64(json_field(
            json_field(json_field(&json, "api_hits"), "module.cpu.count"),
            "tests"
        )) > 0
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn xsht_test_cov_json_counts_example_runs_as_examples() {
    let path = temp_path("xsht-example-cov-json").with_extension("json");
    let _ = std::fs::remove_file(&path);

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args([
            "test",
            "--examples",
            "--exact",
            "examples::release-package",
            "--cov-json",
            path.to_str().unwrap(),
        ])
        .output()
        .expect("run xsht");

    assert!(output.status.success(), "{:?}", output);
    let json = json_parse(&std::fs::read_to_string(&path).unwrap());
    let print_hits = json_field(json_field(&json, "api_hits"), "core.print");
    assert_eq!(json_u64(json_field(print_hits, "tests")), 0);
    assert!(json_u64(json_field(print_hits, "examples")) > 0);
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
        if path.file_name().is_some_and(|name| name == "IDEAS.md") {
            assert!(path.is_file(), "showcase/IDEAS.md must be a file");
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

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["test"])
        .env("CARGO_BIN_EXE_xsh", env!("CARGO_BIN_EXE_xsh"))
        .env("CARGO_BIN_EXE_xsht", env!("CARGO_BIN_EXE_xsht"))
        .env(
            "CARGO_BIN_EXE_xsh-test-sleeper",
            env!("CARGO_BIN_EXE_xsh-test-sleeper"),
        )
        .output()
        .expect("run showcase tests");

    assert!(
        output.status.success(),
        "xsh native tests\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
