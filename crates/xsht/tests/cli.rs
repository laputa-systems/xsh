#![allow(clippy::single_call_fn)]

use std::fs;
use std::process::Command;
use std::sync::Mutex;
use tempfile::TempDir;

static SIGNAL_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn xsht_top_level_help_lists_subcommands_only() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .arg("-h")
        .output()
        .expect("run xsht help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Commands:"));
    assert!(stdout.contains("lint"));
    assert!(stdout.contains("Run `xsht COMMAND --help`"));
    assert!(!stdout.contains("--runless"));
    assert!(!stdout.contains("--trace-format"));
}

#[test]
fn xsht_lint_help_is_subcommand_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["lint", "--help"])
        .output()
        .expect("run xsht lint help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Usage:\n  xsht lint"));
    assert!(stdout.contains("--fix"));
    assert!(stdout.contains("--runless"));
    assert!(!stdout.contains("xsht trace"));
}

#[test]
fn xsht_lint_short_help_is_accepted() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["lint", "-h"])
        .output()
        .expect("run xsht lint short help");

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("xsht lint [--fix] [--runless] [FILE...]")
    );
}

#[test]
fn fmt_uses_nearest_xsht_config_line_width() {
    let root = TempDir::new().expect("create temp root");
    let narrow = root.path().join("narrow");
    fs::create_dir_all(&narrow).expect("create narrow dir");
    fs::write(
        root.path().join("xsht-config.ini"),
        "[format]\nline-width = 120\n",
    )
    .expect("write root config");
    fs::write(
        narrow.join("xsht-config.ini"),
        "[format]\nline-width = 60\n",
    )
    .expect("write narrow config");
    let script = narrow.join("main.xsh");
    fs::write(
        &script,
        "let values = [\"alpha\", \"beta\", \"gamma\", \"delta\", \"epsilon\", \"zeta\"]\n",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["fmt", script.to_str().unwrap()])
        .current_dir(root.path())
        .output()
        .expect("run xsht fmt");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let formatted = fs::read_to_string(&script).expect("read formatted script");
    assert_eq!(
        formatted,
        "let values = [\n  \"alpha\",\n  \"beta\",\n  \"gamma\",\n  \"delta\",\n  \"epsilon\",\n  \"zeta\",\n]\n"
    );
}

#[test]
fn fmt_reports_invalid_xsht_config_line_width() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("xsht-config.ini"),
        "[format]\nline-width = nope\n",
    )
    .expect("write config");
    let script = root.path().join("main.xsh");
    fs::write(&script, "let value = 1\n").expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["fmt", "--check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht fmt");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("format.line-width"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fmt_ignores_legacy_config_ini() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("config.ini"),
        "[format]\nline-width = 60\n",
    )
    .expect("write legacy config");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "let values = [\"alpha\", \"beta\", \"gamma\", \"delta\", \"epsilon\", \"zeta\"]\n",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["fmt", "--check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht fmt");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_annotate_uses_xsht_config_line_width() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("xsht-config.ini"),
        "[format]\nline-width = 60\n",
    )
    .expect("write config");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc local(input = Path(\".\"), source = Path(\".\"), destination = Path(\".\")) {}\n",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "--annotate", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let annotated = fs::read_to_string(&script).expect("read annotated script");
    assert_eq!(
        annotated,
        "proc local(\n  input: Path = Path(\".\"),\n  source: Path = Path(\".\"),\n  destination: Path = Path(\".\"),\n) {}\n"
    );
}

#[test]
fn check_reports_compact_lowerability_by_default() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc fallible() [error] -> Result[Str] {
  return \"ok\"
}

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  with value = fallible() {
    print ${value}
  } else |err| {
    return Err(err)
  }
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("compact.unlowered-main"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("proc main could not be lowered: unsupported statement in body"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("unsupported statement in body"),
        "stderr: {stderr}"
    );
}

#[test]
fn check_explicit_directory_uses_directory_config() {
    let root = TempDir::new().expect("create temp root");
    let project = root.path().join("project");
    fs::create_dir_all(&project).expect("create project dir");
    fs::write(
        root.path().join("xsht-config.ini"),
        "exclude = project/bad.xsh\n",
    )
    .expect("write root config");
    fs::write(project.join("xsht-config.ini"), "").expect("write project config");
    fs::write(project.join("bad.xsh"), "let value =\n").expect("write bad script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "project"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("parse.expected-expression"),
        "stderr: {stderr}"
    );
}

#[test]
fn check_summary_groups_directory_failures_by_code() {
    let root = TempDir::new().expect("create temp root");
    let project = root.path().join("project");
    fs::create_dir_all(&project).expect("create project dir");
    fs::write(project.join("parse.xsh"), "let value =\n").expect("write parse script");
    fs::write(
        project.join("lower.xsh"),
        "proc main(...argv: List[Str]) [error] -> Result[Unit] {
  with value = fallible() {
    print ${value}
  } else |err| {
    return Err(err)
  }
  return Ok()
}

proc fallible() [error] -> Result[Str] {
  return \"ok\"
}
",
    )
    .expect("write lowerability script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "--summary", "project"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("parse.expected-expression"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("compact.unlowered-main"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("xsht check summary:"), "stderr: {stderr}");
    assert!(
        stderr.contains("parse.expected-expression: 1"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("compact.unlowered-main: 1"),
        "stderr: {stderr}"
    );
}

#[test]
fn check_directory_lowerability_failure_exits_nonzero() {
    let root = TempDir::new().expect("create temp root");
    let project = root.path().join("project");
    fs::create_dir_all(&project).expect("create project dir");
    fs::write(project.join("ok.xsh"), "let value = 1\n").expect("write ok script");
    fs::write(
        project.join("bad.xsh"),
        "proc main(...argv: List[Str]) [error] -> Result[Unit] {
  with value = fallible() {
    print ${value}
  } else |err| {
    return Err(err)
  }
  return Ok()
}

proc fallible() [error] -> Result[Str] {
  return \"ok\"
}
",
    )
    .expect("write bad script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "project"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("compact.unlowered-main"),
        "stderr: {stderr}"
    );
}

#[test]
fn check_top_level_user_imports_are_skippable_for_lowerability() {
    let root = TempDir::new().expect("create temp root");
    let project = root.path().join("project");
    fs::create_dir_all(project.join("pm")).expect("create module dir");
    fs::write(project.join("helper.xsh"), "export let value = 1\n").expect("write helper");
    fs::write(project.join("pm").join("make.xsh"), "export let jobs = 1\n")
        .expect("write pm module");
    fs::write(
        project.join("PKGBUILD-shared.xsh"),
        "export let pkgname = \"demo\"\n",
    )
    .expect("write hyphen module");
    fs::write(
        project.join("main.xsh"),
        "use helper as h
use pm.make as make
use PKGBUILD-shared as PKGBUILD_shared
",
    )
    .expect("write main script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "project/main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_compact_lowerability_reports_dependency_blocker() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "pure helper(x: Int = 1 + 1) -> Int {
  return x
}

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let _ = helper()
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("compact.unlowered-main"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains(
            "proc main could not be lowered because helper has an unsupported parameter default"
        ),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("proc main requires compact lowering"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("unsupported parameter default"),
        "stderr: {stderr}"
    );
}

#[test]
fn check_top_level_lowerability_reports_first_nested_call_blocker() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "pure scan_corpus(x: Int = 1 + 1) -> Int {
  return x
}

let report = {corpus: scan_corpus()}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("compact.unlowered-statement"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("first blocker: call `scan_corpus`"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("first unsupported lowered construct: call `scan_corpus`"),
        "stderr: {stderr}"
    );
}

#[test]
fn check_main_dependency_lowers_result_context_chain() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "error AppletError = Usage(message: Str)

pure common_int(raw: Str) -> Result[Int] {
  match raw {
    \"1\" => 1
    _ => raw.parse_int().context(\"usage\", \"bad int\")?
  }
}

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let _ = common_int(\"2\")?
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_for_line_item_type_allows_lowered_str_methods() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let text = \"a=b\\nc=d\"
  for line in text.lines() {
    let parts = line.split(\"=\")
    print ${parts.len()}
  }
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_local_method_chain_types_flow_through_if_binding() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "pure lookup(body: Str, name: Str) -> Str {
  for raw in body.lines() {
    let stripped = raw.trim()
    let line = if stripped.starts_with(\"export \") { stripped.split(\"export \").get(1, \"\").trim() } else { stripped }
    if line.starts_with(f\"${name}=\") {
      return line.split(\"=\").get(1, \"\").trim().replace(\"\\\"\", \"\").replace(\"'\", \"\")
    }
  }
  return \"\"
}

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let _ = lookup(\"export A=1\", \"A\")
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn check_match_ok_binding_type_allows_lowered_str_methods() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc read_summary(candidate: Path) [fs, error] -> Result[Str] {
  match fs.read_text(candidate) {
    Ok(text_value) => {
      let lines = text_value.lines().collect()
      let summary = lines[1].trim()
      return summary
    }
    Err(_) => {}
  }
  \"\"
}

proc main(...argv: List[Str]) [fs, error] -> Result[Unit] {
  let _ = read_summary(path.absolute(\"x\")?)?
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_run_text_binding_type_allows_lowered_str_methods() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc main(...argv: List[Str]) [process, error] -> Result[Unit] {
  let out = run.text printf hello ?
  print ${out.trim()}
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_explicit_list_annotation_survives_any_result_binding() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let stored: Record = {deps: []}
  let deps: List[Str] = stored.get(\"deps\")?
  print \"deps\" deps.len() deps.join(\" \")
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_local_binding_can_shadow_import_capture_for_lowerability() {
    let root = TempDir::new().expect("create temp root");
    fs::write(root.path().join("remote.xsh"), "export let value = 1\n").expect("write module");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "use remote

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let remote = \"local\"
  print ${remote.trim()}
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_par_map_result_item_type_flows_to_for_loop() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "type BuiltPackage = {metadata_sha256: Str}
type Package = {name: Str}

proc build_world_package(pkg: Package) [error] -> Result[List[BuiltPackage]] {
  return Ok([{metadata_sha256: pkg.name}])
}

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let pending: List[Package] = [{name: \"demo\"}]
  let built_batches = pending |> par-map --jobs=1 { |pkg| build_world_package(pkg) }
  for built in built_batches {
    print ${built.len()}
  }
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_where_pipeline_preserves_item_type_for_loop() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let words = \"a b\".split(\" \") |> where .trim() != \"\"
  for word in words {
    print ${word.trim()}
  }
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_path_property_field_type_flows_to_method_call() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc main(...argv: List[Str]) [fs, error] -> Result[Unit] {
  let dir = fs.cwd()?
  let parent = dir.parent
  print ${parent.display()}
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_any_record_get_can_be_narrowed_by_binding_annotation() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let exports: Any = {sources: [\"a\", \"b\"]}
  if exports.has(\"sources\") {
    let sources: List[Str] = exports.get(\"sources\")?
    print ${sources.len()}
  }
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_fs_walk_map_path_result_type_flows_to_for_loop() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc main(...argv: List[Str]) [fs, error] -> Result[Unit] {
  let dest = p\".\"
  let manifest = fs.walk(dest)
    |> where .kind == \"file\" or .kind == \"symlink\"
    |> map { |entry|
      entry.path.strip_prefix(dest)?
    }
    |> sort-by .display()
  for rel_path in manifest {
    let key = rel_path.display()
    print ${key}
  }
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_compact_lowerability_rejects_unsupported_lowered_record_method() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let exports: Record = {sources: {name: \"demo\"}}
  let sources = exports.get(\"sources\")?
  if sources.len() != 0 {
    return Ok()
  }
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(env!("CARGO_BIN_EXE_xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("compact.unlowered-main"),
        "stderr: {stderr}"
    );
}

#[test]
fn lint_returns_interrupted_status_for_pending_sigint() {
    let _lock = SIGNAL_TEST_LOCK.lock().unwrap();
    let _guard = xsh::runtime::process::install_cancellation_signal_handlers()
        .expect("install cancellation signal handlers");
    xsh::runtime::process::clear_cancellation_request();
    let kill_result = unsafe { libc::kill(libc::getpid(), libc::SIGINT) };
    assert_eq!(kill_result, 0);

    let output = xsht::cli::lint_files(&["unused.xsh".to_string()], false, false);
    xsh::runtime::process::clear_cancellation_request();

    assert_eq!(output.status, 130);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("interrupted by SIGINT")
    );
}
