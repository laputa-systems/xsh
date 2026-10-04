#![allow(clippy::single_call_fn)]

use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::process::Command;
use std::sync::Mutex;
use tempfile::TempDir;

static SIGNAL_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn cli_workers_check_and_lint_nested_schema_constructors_without_stack_environment() {
    let root = TempDir::new().expect("nested schema worker fixture");
    let script = root.path().join("nested.xsh");
    let mut source = "type Leaf = {value: Int}\n".to_string();
    let mut previous = "Leaf".to_string();
    let mut constructor = "Leaf(value: 1)".to_string();
    for depth in 0..40 {
        let name = format!("Layer{depth}");
        source.push_str(&format!("type {name} = {{child: {previous}}}\n"));
        constructor = format!("{name}(child: {constructor})");
        previous = name;
    }
    source.push_str(&format!("pure build() -> {previous} {{ {constructor} }}\nlet value = build()\nlet _ = value\n"));
    fs::write(&script, &source).expect("write nested constructors");
    for command in ["check", "lint"] {
        let output = Command::new(release_bin!("xsht"))
            .args([command, "nested.xsh"])
            .current_dir(root.path())
            .env_remove("RUST_MIN_STACK")
            .env_remove("XSH_MODULE_PATH")
            .output().expect("run nested constructor tooling");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(if command == "check" { output.status.success() }
            else { matches!(output.status.code(), Some(0 | 1)) },
            "{command}: {:?}\n{stderr}", output.status);
        assert!(!stderr.contains("stack overflow"), "{command}: {stderr}");
        assert_eq!(fs::read_to_string(&script).unwrap(), source);
    }
}

#[test]
fn mixed_enum_and_record_require_migration_rechecks_import_graph_and_converges_in_stages() {
    let root = TempDir::new().expect("mixed migration fixture");
    let entry = root.path().join("entry.xsh");
    let module = root.path().join("choice.xsh");
    fs::write(&module, "##! Choices.\n## A nominal choice.\nexport type Choice = Selected(Int) | Empty # retained café\n").unwrap();
    fs::write(&entry, "use choice as c\n## A name.\nexport type Name = {name: Str}\nlet _ = record.require({name: \"café\"}, {name: \"Str\"})? # retained receiver\nlet choice: c.Choice = c.Selected(7)\nprint \"café\"\nmatch choice { c.Selected(number) => print $number; c.Empty => print \"empty\" }\n").unwrap();
    let run = |arguments: &[&str]| Command::new(release_bin!("xsht"))
        .args(arguments).current_dir(root.path()).output().unwrap();
    let before = run(&["check", "entry.xsh"]);
    assert!(!before.status.success());
    let first = run(&["lint", "--fix", "entry.xsh"]);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    let fixed_entry = fs::read_to_string(&entry).unwrap();
    let fixed_module = fs::read_to_string(&module).unwrap();
    assert!(fixed_entry.contains("{name: \"café\"}.require(Name)? # retained receiver"), "{fixed_entry}");
    assert!(fixed_module.contains("export enum Choice {"), "{fixed_module}");
    assert!(fixed_module.contains("# retained café"), "{fixed_module}");
    let checked = run(&["check", "entry.xsh"]);
    assert!(checked.status.success(), "{}", String::from_utf8_lossy(&checked.stderr));
    let executed = run(&["trace", "entry.xsh"]);
    assert!(executed.status.success(), "{}", String::from_utf8_lossy(&executed.stderr));
    assert_eq!(String::from_utf8_lossy(&executed.stdout), "café\n7\n");
    // Exact syntax/API repair makes ordinary lints available on the next pass;
    // they can then remove identity schema validation and normalize layout.
    let second = run(&["lint", "--fix", "entry.xsh"]);
    assert!(second.status.success(), "{}", String::from_utf8_lossy(&second.stderr));
    let canonical_entry = fs::read_to_string(&entry).unwrap();
    let canonical_module = fs::read_to_string(&module).unwrap();
    assert!(canonical_entry.contains("# retained receiver"), "{canonical_entry}");
    assert!(canonical_module.contains("# retained café"), "{canonical_module}");
    let after = run(&["trace", "entry.xsh"]);
    assert!(after.status.success(), "{}", String::from_utf8_lossy(&after.stderr));
    assert_eq!(executed.stdout, after.stdout);
    let third = run(&["lint", "--fix", "entry.xsh"]);
    assert!(third.status.success(), "{}", String::from_utf8_lossy(&third.stderr));
    assert_eq!(canonical_entry, fs::read_to_string(&entry).unwrap());
    assert_eq!(canonical_module, fs::read_to_string(&module).unwrap());
}

#[test]
fn mixed_enum_and_record_require_migration_refuses_unproved_identity() {
    let root = TempDir::new().expect("unproved mixed migration fixture");
    let entry = root.path().join("entry.xsh");
    let module = root.path().join("choice.xsh");
    let module_source = "##! Choices.\n## A nominal choice.\nexport type Choice = Selected(Int) | Empty\n";
    let entry_source = "use choice as c\ntype Name = {name: Str}\nlet _ = record.require({name: 7}, {name: \"Str\"})?\nlet choice: c.Choice = c.Selected(7)\n";
    fs::write(&entry, entry_source).unwrap();
    fs::write(&module, module_source).unwrap();
    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "--fix", "entry.xsh"]).current_dir(root.path()).output().unwrap();
    assert!(!output.status.success());
    let diagnostic_text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(diagnostic_text.contains("lint.removed-record-require"), "{diagnostic_text}");
    assert_eq!(entry_source, fs::read_to_string(&entry).unwrap());
    assert_eq!(module_source, fs::read_to_string(&module).unwrap());
}

#[test]
fn mixed_enum_and_record_require_migration_refuses_unrelated_import_graph_errors() {
    for (broken_module, consumed_result) in [(false, false), (true, false), (false, true)] {
        let root = TempDir::new().expect("rejected mixed migration fixture");
        let entry = root.path().join("entry.xsh");
        let module = root.path().join("choice.xsh");
        let mut module_source = "##! Choices.\n## A nominal choice.\nexport type Choice = Selected(Int) | Empty # retained café\n".to_string();
        let mut entry_source = "use choice as c\ntype Name = {name: Str}\nlet _ = record.require({name: \"café\"}, {name: \"Str\"})?\nlet choice: c.Choice = c.Selected(7)\nprint \"café\"\n".to_string();
        let expected_code = if consumed_result {
            // A removed API has no executable result type. Its consumer errors
            // remain checker failures, even when the call has an identity fix.
            entry_source = entry_source.replace("let _ =", "let value =");
            entry_source.push_str("print $value.name\n");
            "check.field-access"
        } else {
            if broken_module { module_source.push_str("let broken: Int = \"wrong\"\n"); }
            else { entry_source.push_str("let broken: Int = \"wrong\"\n"); }
            "check.type-mismatch"
        };
        fs::write(&entry, &entry_source).unwrap();
        fs::write(&module, &module_source).unwrap();
        let output = Command::new(release_bin!("xsht"))
            .args(["lint", "--fix", "entry.xsh"]).current_dir(root.path()).output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected_code), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(entry_source, fs::read_to_string(&entry).unwrap());
        assert_eq!(module_source, fs::read_to_string(&module).unwrap());
    }
}

#[test]
fn removed_record_require_cli_fix_rechecks_and_converges_in_stages() {
    let root = TempDir::new().expect("record migration fixture");
    let entry = root.path().join("entry.xsh");
    fs::write(&entry, "export type Name = {name: Str}\nconst required = {name: \"Str\"}\nlet value = record.require({name: \"café\", extra: 7}, required)?\nprint $value.name\n").unwrap();
    let run = |arguments: &[&str]| Command::new(release_bin!("xsht"))
        .args(arguments).current_dir(root.path()).output().unwrap();
    let before = run(&["check", "entry.xsh"]);
    assert_eq!(before.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&before.stderr).contains("check.removed-record-require"));
    let first = run(&["lint", "--fix", "entry.xsh"]);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    let fixed = fs::read_to_string(&entry).unwrap();
    assert!(!fixed.contains("record.require"), "{fixed}");
    assert!(fixed.contains("café") || fixed.contains("caf\\u{e9}"));
    let after = run(&["check", "entry.xsh"]);
    assert!(after.status.success(), "{}", String::from_utf8_lossy(&after.stderr));
    let before_ordinary_fixes = run(&["trace", "entry.xsh"]);
    assert!(before_ordinary_fixes.status.success(), "{}", String::from_utf8_lossy(&before_ordinary_fixes.stderr));
    let second = run(&["lint", "--fix", "entry.xsh"]);
    assert!(second.status.success(), "{}", String::from_utf8_lossy(&second.stderr));
    let canonical = fs::read_to_string(&entry).unwrap();
    let third = run(&["lint", "--fix", "entry.xsh"]);
    assert!(third.status.success(), "{}", String::from_utf8_lossy(&third.stderr));
    assert_eq!(canonical, fs::read_to_string(&entry).unwrap());
    let executed = run(&["trace", "entry.xsh"]);
    assert!(executed.status.success(), "{}", String::from_utf8_lossy(&executed.stderr));
    assert_eq!(before_ordinary_fixes.stdout, executed.stdout);
    assert!(String::from_utf8_lossy(&executed.stdout).contains("café"));
}

#[test]
fn removed_record_require_cli_fix_preserves_unrelated_errors_and_comments() {
    for source in [
        "type Name = {name: Str}\nlet value = record.require({name: \"demo\"}, {name: \"Str\"})?\nlet broken: Int = \"wrong\"\n",
        "type Name = {name: Str}\nlet value = record.require({name: \"demo\"}, # retained café\n {name: \"Str\"})?\n",
    ] {
        let root = TempDir::new().unwrap();
        let entry = root.path().join("entry.xsh");
        fs::write(&entry, source).unwrap();
        let output = Command::new(release_bin!("xsht"))
            .args(["lint", "--fix", "entry.xsh"]).current_dir(root.path()).output().unwrap();
        assert!(!output.status.success());
        assert_eq!(source, fs::read_to_string(&entry).unwrap());
    }
}

#[test]
fn check_strict_option_reports_default_dynamic_policy_before_loading() {
    let root = TempDir::new().expect("temporary option fixture");
    let output = Command::new(release_bin!("xsht"))
        .args(["check", "--strict", "missing.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run removed check option");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 option diagnostic");
    assert!(stderr.contains("`xsht check --strict` was removed"), "{stderr}");
    assert!(stderr.contains("dynamic boundaries are checked by default"), "{stderr}");
    assert!(!stderr.contains("failed to read"), "{stderr}");

    let help = Command::new(release_bin!("xsht"))
        .args(["check", "--help"])
        .output()
        .expect("run check help");
    assert!(help.status.success());
    assert!(!String::from_utf8_lossy(&help.stdout).contains("--strict"));
}

#[test]
fn check_dynamic_boundary_rejects_disk_fixture_without_annotation_writes() {
    let root = TempDir::new().expect("temporary dynamic boundary fixture");
    let source = include_str!("../../../tests/fixtures/sema/invalid/unchecked-json-boundary.xsh");
    let fixture = root.path().join("boundary.xsh");
    fs::write(&fixture, source).expect("copy boundary fixture");
    let output = Command::new(release_bin!("xsht"))
        .args(["check", "--annotate", "boundary.xsh"])
        .current_dir(root.path())
        .output()
        .expect("check dynamic boundary fixture");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("err[check.dynamic-boundary]"));
    assert_eq!(fs::read_to_string(fixture).expect("read rejected fixture"), source);
}

#[test]
fn lint_fix_converges_when_tail_edits_contain_named_argument_edits() {
    let root = TempDir::new().expect("temporary lint fixture");
    let fixture = root.path().join("fixture.xsh");
    fs::write(&fixture, include_str!("../../../tests/fixtures/syntax/valid/ergonomics-fix-convergence.xsh"))
        .expect("write lint fixture");
    let run = |arguments: &[&str]| Command::new(release_bin!("xsht"))
        .args(arguments)
        .current_dir(root.path())
        .output()
        .expect("run isolated xsht fixture");
    let before = run(&["trace", "fixture.xsh"]);
    assert!(before.status.success(), "{}", String::from_utf8_lossy(&before.stderr));
    let first = run(&["lint", "--fix", "fixture.xsh"]);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    let fixed = fs::read_to_string(&fixture).expect("read first fix");
    let second = run(&["lint", "--fix", "fixture.xsh"]);
    assert!(second.status.success(), "{}", String::from_utf8_lossy(&second.stderr));
    assert_eq!(fixed, fs::read_to_string(&fixture).expect("read second fix"));
    assert!(fixed.contains("words.join(separator:)"), "{fixed}");
    let after = run(&["trace", "fixture.xsh"]);
    assert!(after.status.success(), "{}", String::from_utf8_lossy(&after.stderr));
    assert_eq!(before.stdout, after.stdout);
}

#[test]
fn xsht_reports_non_utf8_argument_without_panicking() {
    let raw_path = std::ffi::OsString::from_vec(b"raw\xffpath.xsh".to_vec());
    let output = Command::new(release_bin!("xsht"))
        .arg("check")
        .arg(raw_path)
        .output()
        .expect("run xsht");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "xsht: argument 2 is not valid UTF-8\n"
    );
}

#[test]
fn xsht_top_level_help_is_a_complete_hybrid_reference() {
    let output = Command::new(release_bin!("xsht"))
        .arg("-h")
        .output()
        .expect("run xsht help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("xsht -h | --help"));
    assert!(stdout.contains("Start here:"));
    assert!(stdout.contains("Command reference:"));
    assert!(stdout.contains("lint — Run quality checks and optional fixes"));
    assert!(stdout.contains("--runless"));
    assert!(stdout.contains("--trace-format FORMAT"));
    assert!(stdout.contains("--cov-json FILE"));
    assert!(stdout.contains("xsht grep 'X.len()' ."));
    assert!(!stdout.contains("Run `xsht COMMAND --help`"));

    for command in [
        "check", "fmt", "lint", "ast", "trace", "api", "test", "grep", "refactor",
    ] {
        assert!(
            stdout.contains(&format!("{command} —")),
            "missing {command} help"
        );
    }

    let grep = stdout.find("grep —").expect("grep section");
    let refactor = stdout.find("refactor —").expect("refactor section");
    let grep_example = stdout.find("xsht grep 'X.len()' .").expect("grep example");
    assert!(grep < grep_example && grep_example < refactor);
}

#[test]
fn xsht_lint_help_is_subcommand_specific() {
    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "--help"])
        .output()
        .expect("run xsht lint help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("xsht lint — Run quality checks and optional fixes"));
    assert!(stdout.contains("Usage:\n  xsht lint"));
    assert!(stdout.contains("--fix"));
    assert!(stdout.contains("--runless"));
    assert!(!stdout.contains("xsht trace"));
}

#[test]
fn xsht_grep_help_keeps_examples_with_grep() {
    let output = Command::new(release_bin!("xsht"))
        .args(["grep", "--help"])
        .output()
        .expect("run xsht grep help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("xsht grep — Search scripts with AST patterns"));
    assert!(stdout.contains("xsht grep 'X.len()' ."));
    assert!(stdout.contains("xsht grep 'for NAME in ITER' ."));
    assert!(!stdout.contains("xsht refactor"));
}

#[test]
fn xsht_help_topic_uses_the_generated_command_catalog() {
    let output = Command::new(release_bin!("xsht"))
        .args(["help", "grep"])
        .output()
        .expect("run xsht help grep");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("xsht grep — Search scripts with AST patterns"));
    assert!(stdout.contains("xsht grep 'X.len()' ."));
    assert!(!stdout.contains("Command reference:"));
}

#[test]
fn xsht_test_help_lists_parallelism_option() {
    let output = Command::new(release_bin!("xsht"))
        .args(["test", "--help"])
        .output()
        .expect("run xsht test help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("xsht test [OPTIONS] [FILTER]"));
    assert!(stdout.contains("--jobs N"));
    assert!(stdout.contains("--api"));
    assert!(!stdout.contains("--examples"));
    assert!(!stdout.contains("--all"));
}

#[test]
fn xsht_lint_short_help_is_accepted() {
    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "-h"])
        .output()
        .expect("run xsht lint short help");

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("xsht lint [--fix] [--runless] [--only RULE[,RULE...]] [FILE...]")
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

    let output = Command::new(release_bin!("xsht"))
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
fn lint_accepts_a_cli_main_entry_beside_other_entry_files() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("tool.xsh"),
        "cli main(name: Str) {\n  print $name\n}\n",
    )
    .expect("write cli entry");
    fs::write(root.path().join("other.xsh"), "print \"other\"\n").expect("write other entry");

    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "."])
        .current_dir(root.path())
        .output()
        .expect("run xsht lint");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn lint_list_jsonl_names_every_selectable_code_with_a_summary() {
    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "--list", "--format", "jsonl"])
        .output()
        .expect("run xsht lint --list");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8 listing");
    let lines = stdout.lines().collect::<Vec<_>>();
    assert!(lines.len() > 50, "{stdout}");
    assert!(
        lines
            .iter()
            .all(|line| line.starts_with("{\"code\":\"") && line.contains("\",\"summary\":\""))
    );
    assert!(stdout.contains("{\"code\":\"lint.prefer-guard\",\"summary\":"));
    assert!(stdout.contains("{\"code\":\"check.bool-statement\",\"summary\":"));

    let rejected = Command::new(release_bin!("xsht"))
        .args(["lint", "--list", "--fix"])
        .output()
        .expect("run xsht lint --list --fix");
    assert_eq!(rejected.status.code(), Some(2));
}

#[test]
fn fmt_discovery_skips_format_excludes_but_formats_named_files() {
    let root = TempDir::new().expect("create temp root");
    let snippets = root.path().join("snippets");
    fs::create_dir_all(&snippets).expect("create snippets dir");
    fs::write(
        root.path().join("xsht-config.ini"),
        "[format]\nexclude = snippets/**\n",
    )
    .expect("write config");
    let unformatted = "let  value = 1\n";
    let snippet = snippets.join("one.xsh");
    fs::write(&snippet, unformatted).expect("write snippet");
    fs::write(root.path().join("main.xsh"), unformatted).expect("write main");

    let discovered = Command::new(release_bin!("xsht"))
        .args(["fmt", "--check"])
        .current_dir(root.path())
        .output()
        .expect("run xsht fmt --check");
    let stdout = String::from_utf8_lossy(&discovered.stdout);
    assert_eq!(discovered.status.code(), Some(1), "stdout: {stdout}");
    assert!(
        stdout.contains("main.xsh: needs formatting"),
        "stdout: {stdout}"
    );
    assert!(!stdout.contains("one.xsh"), "stdout: {stdout}");

    let named = Command::new(release_bin!("xsht"))
        .args(["fmt", "--check", "snippets/one.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht fmt --check on a named file");
    assert_eq!(named.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&named.stdout).contains("one.xsh: needs formatting"));
}

#[test]
fn fmt_explicit_directory_formats_xsh_files() {
    let root = TempDir::new().expect("create temp root");
    let project = root.path().join("project");
    fs::create_dir_all(project.join("nested")).expect("create project dirs");
    fs::write(project.join("main.xsh"), "let values=[1,2,3]\n").expect("write main script");
    fs::write(
        project.join("nested").join("helper.xsh"),
        "let values=[4,5,6]\n",
    )
    .expect("write nested script");

    let output = Command::new(release_bin!("xsht"))
        .args(["fmt", project.to_str().unwrap()])
        .current_dir(root.path())
        .output()
        .expect("run xsht fmt");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(project.join("main.xsh")).expect("read main script"),
        "let values = [1, 2, 3]\n"
    );
    assert_eq!(
        fs::read_to_string(project.join("nested").join("helper.xsh")).expect("read nested script"),
        "let values = [4, 5, 6]\n"
    );
}

#[test]
fn fmt_checks_imported_modules() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("helper.xsh"),
        "##! Invalid helper module.\n## Deliberately returns the wrong type.\nexport pure bad() -> Int {\n  return \"not an int\"\n}\n",
    )
    .expect("write helper module");
    fs::write(
        root.path().join("main.xsh"),
        "use helper\nprint helper.bad()\n",
    )
    .expect("write main script");

    let output = Command::new(release_bin!("xsht"))
        .args(["fmt", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht fmt");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("helper.xsh"), "stderr: {stderr}");
    assert!(stderr.contains("check.type-mismatch"), "stderr: {stderr}");
}

#[test]
fn copied_xsht_formats_and_lints_script_backed_calls_in_static_and_loaded_modules() {
    let root = TempDir::new().expect("create isolated source root");
    let xsht = root.path().join("xsht");
    fs::copy(release_bin!("xsht"), &xsht).expect("copy xsht");
    fs::write(
        root.path().join("helper.xsh"),
        "##! Static helper.\n## Return a terminal sequence.\nexport pure color() -> Str { tui.red() }\n",
    )
    .expect("write static module");
    fs::write(
        root.path().join("dynamic.xsh"),
        "##! Dynamic helper.\n## Return a terminal sequence.\nexport pure color() -> Str { tui.bold() }\n",
    )
    .expect("write loaded module");
    fs::write(
        root.path().join("main.xsh"),
        "use helper\ntype Loaded = module { export pure color() -> Str }\nproc main() [fs, io, error] {\n  let loaded = module.load(p\"dynamic.xsh\")?.require(Loaded)?\n  let both = helper.color() + loaded.color()\n  print $both\n}\n",
    )
    .expect("write entry script");

    for args in [
        vec!["check", "."],
        vec!["fmt", "."],
        vec!["fmt", "--check", "."],
        vec!["lint", "."],
    ] {
        let output = Command::new(&xsht)
            .args(&args)
            .current_dir(root.path())
            .env_remove("XSH_MODULE_PATH")
            .output()
            .expect("run copied xsht");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty(), "{}", args.join(" "));
        assert!(output.stderr.is_empty(), "{}", args.join(" "));
    }
}

#[test]
fn fmt_deduplicates_diagnostics_from_imported_modules() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("helper.xsh"),
        "##! Helper module.\n## This comment is not attached to an export.\nlet value = 1\n\n## Exports a value.\nexport let exported: Int = value\n",
    )
    .expect("write helper module");
    fs::write(
        root.path().join("first.xsh"),
        "use helper\nprint helper.exported\n",
    )
    .expect("write first script");
    fs::write(
        root.path().join("second.xsh"),
        "use helper\nprint helper.exported\n",
    )
    .expect("write second script");

    let output = Command::new(release_bin!("xsht"))
        .args(["fmt", "."])
        .current_dir(root.path())
        .output()
        .expect("run xsht fmt");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("check.orphan-doc-comment").count(),
        1,
        "imported module diagnostic should be rendered once: {stderr}"
    );
}

#[test]
fn lint_explicit_directory_lints_xsh_files() {
    let root = TempDir::new().expect("create temp root");
    let project = root.path().join("project");
    fs::create_dir_all(project.join("nested")).expect("create project dirs");
    fs::write(project.join("main.xsh"), "const value = 1\n").expect("write main script");
    fs::write(project.join("nested").join("helper.xsh"), "const value = 2\n")
        .expect("write helper script");

    let output = Command::new(release_bin!("xsht"))
        .args(["lint", project.to_str().unwrap()])
        .current_dir(root.path())
        .output()
        .expect("run xsht lint");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn lint_fix_deduplicates_diagnostics_from_imported_modules() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("helper.xsh"),
        "##! Helper module.\n## This comment is not attached to an export.\nlet value = 1\n\n## Exports a value.\nexport let exported: Int = value\n",
    )
    .expect("write helper module");
    fs::write(
        root.path().join("first.xsh"),
        "use helper\nprint helper.exported\n",
    )
    .expect("write first script");
    fs::write(
        root.path().join("second.xsh"),
        "use helper\nprint helper.exported\n",
    )
    .expect("write second script");

    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "--fix", "first.xsh", "second.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht lint --fix");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("check.orphan-doc-comment").count(),
        1,
        "repeated imported-module diagnostic: {stderr}"
    );
}

#[test]
fn lint_directory_uses_import_graph_roots() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("helper.xsh"),
        "##! Helper module.\n## This comment is not attached to an export.\nlet value = 1\n\n## Exports a value.\nexport let exported: Int = value\n",
    )
    .expect("write helper module");
    fs::write(
        root.path().join("main.xsh"),
        "use helper\nprint helper.exported\n",
    )
    .expect("write entry script");

    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "."])
        .current_dir(root.path())
        .output()
        .expect("run xsht lint directory");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("check.orphan-doc-comment").count(),
        1,
        "imported module should be linted through its entry root once: {stderr}"
    );
}

#[test]
fn lint_directory_cycle_selects_one_component_root() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("a.xsh"),
        "use b\n##! A module.\n## This comment is not attached to an export.\nlet a = 1\n",
    )
    .expect("write module a");
    fs::write(
        root.path().join("b.xsh"),
        "use a\n##! B module.\n## This comment is not attached to an export.\nlet b = 1\n",
    )
    .expect("write module b");

    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "."])
        .current_dir(root.path())
        .output()
        .expect("run xsht lint cycle");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("parse.module-cycle").count(),
        1,
        "cycle should be reported once through one selected root: {stderr}"
    );
}

#[test]
fn lint_only_restricts_diagnostics_and_fixes_to_named_codes() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    let source = "let name: Str = \"x\"\nprint ${name.byte_len()}\n";
    fs::write(&script, source).expect("write script");
    let lint = |args: &[&str]| Command::new(release_bin!("xsht"))
        .arg("lint").args(args).arg("main.xsh")
        .current_dir(root.path()).output().expect("run xsht lint");
    let codes = |output: &std::process::Output| String::from_utf8_lossy(&output.stderr).lines()
        .filter_map(|line| line.strip_prefix("warn[")?.split(']').next().map(str::to_owned))
        .collect::<Vec<_>>();

    let all = lint(&[]);
    assert_eq!(codes(&all), ["lint.prefer-const", "lint.needless-annotation", "lint.redundant-command-interpolation"]);
    let selected = lint(&["--only", "lint.needless-annotation,lint.redundant-command-interpolation"]);
    assert_eq!(selected.status.code(), Some(1));
    assert_eq!(codes(&selected), ["lint.needless-annotation", "lint.redundant-command-interpolation"]);

    let fixed = lint(&["--only=lint.needless-annotation", "--fix"]);
    assert_eq!(fixed.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&fixed.stderr));
    assert_eq!(fs::read_to_string(&script).expect("read fixed script"), "let name = \"x\"\nprint ${name.byte_len()}\n");
    assert_eq!(codes(&lint(&[])), ["lint.prefer-const", "lint.redundant-command-interpolation"]);
}

#[test]
fn lint_fix_applies_the_bool_statement_assert_fix() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    let source = "let xs = [1, 2]\nxs == [1, 2] # comment stays\nproc note(value: Int) -> Int {\n    print \"eval ${value}\"\n    value\n}\nproc check(n: Int) {\n    match n {\n        1 => note(0) < note(n),\n        _ => {},\n    }\n    note(0) < note(n) < note(3)\n}\ncheck(1)?\ncheck(5)?\n";
    fs::write(&script, source).expect("write script");
    let xsht = |args: &[&str]| Command::new(release_bin!("xsht"))
        .args(args).current_dir(root.path()).output().expect("run xsht");

    let listed = xsht(&["lint", "main.xsh"]);
    assert_eq!(listed.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&listed.stderr);
    assert_eq!(stderr.matches("err[check.bool-statement]").count(), 3, "{stderr}");
    assert!(stderr.contains("help: insert `assert` in a braced match arm -> { assert note(0) < note(n) }"), "{stderr}");

    let fixed = xsht(&["lint", "--fix", "main.xsh"]);
    assert_eq!(fixed.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&fixed.stderr));
    let migrated = fs::read_to_string(&script).expect("read fixed script");
    for statement in ["assert xs == [1, 2] # comment stays", "1 => assert note(0) < note(n)", "  assert note(0) < note(n) < note(3)"] {
        assert!(migrated.contains(statement), "missing {statement:?}: {migrated}");
    }
    assert_eq!(xsht(&["lint", "--fix", "main.xsh"]).status.code(), Some(0));
    assert_eq!(fs::read_to_string(&script).expect("reread fixed script"), migrated, "a second fix makes no change");

    let traced = xsht(&["trace", "main.xsh"]);
    assert_eq!(traced.status.code(), Some(3));
    assert_eq!(String::from_utf8_lossy(&traced.stdout).lines().take(8).collect::<Vec<_>>(),
        ["eval 0", "eval 1", "eval 0", "eval 1", "eval 3", "eval 0", "eval 5", "eval 3"], "operands evaluate once, in order");
    let stderr = String::from_utf8_lossy(&traced.stderr);
    assert!(stderr.contains("assertion failed: note(0) < note(n) < note(3)\nordering comparison failed: 5 < 3"), "{stderr}");
}

#[test]
fn lint_only_bool_statement_applies_only_its_assert_fix() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(&script, "let name: Str = \"x\"\nname == \"x\"\n").expect("write script");
    let fixed = Command::new(release_bin!("xsht"))
        .args(["lint", "--only", "check.bool-statement", "--fix", "main.xsh"])
        .current_dir(root.path()).output().expect("run xsht lint");
    assert_eq!(fixed.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&fixed.stderr));
    assert_eq!(fs::read_to_string(&script).expect("read fixed script"), "let name: Str = \"x\"\nassert name == \"x\"\n");
}

/// A scoped fix rewrites only its diagnosed spans; unformatted code elsewhere
/// keeps its exact bytes.
#[test]
fn lint_only_fix_leaves_bytes_outside_edited_spans() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    let source = "let xs = [1,2,3]\nlet total: Int = xs.len()\nprint   f\"{total}\"  \nlet ys=[1, 2]\r\nprint f\"{ys.len()}\"\n";
    for (rule, expected) in [
        ("lint.needless-annotation", source.replace("let total: Int =", "let total =")),
        ("lint.prefer-const", source.replace("let xs", "const xs").replace("let ys", "const ys")),
    ] {
        fs::write(&script, source).expect("write script");
        let fixed = Command::new(env!("CARGO_BIN_EXE_xsht"))
            .args(["lint", "--only", rule, "--fix", "main.xsh"])
            .current_dir(root.path()).output().expect("run xsht lint");
        assert_eq!(fixed.status.code(), Some(0), "{rule}: {}", String::from_utf8_lossy(&fixed.stderr));
        assert_eq!(fs::read_to_string(&script).expect("read fixed script"), expected, "{rule}");
    }
}

#[test]
fn lint_only_rejects_unknown_codes() {
    for args in [&["lint", "--only", "lint.prefer-const,lint.no-such-rule", "."][..], &["lint", "--only"][..]] {
        let output = Command::new(release_bin!("xsht")).args(args).output().expect("run xsht lint");
        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("--only"), "{stderr}");
    }
    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "--only", "check.unresolved-name", "."]).output().expect("run xsht lint");
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown lint rule 'check.unresolved-name'"));
}

#[test]
fn lint_fix_applies_an_imported_module_edit_once() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("helper.xsh"),
        "##! Helper module.\n## Exports a value.\nexport let value: Int = 1\n",
    )
    .expect("write helper module");
    fs::write(
        root.path().join("main.xsh"),
        "use helper\nprint helper.value\n",
    )
    .expect("write entry script");

    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "--fix", "."])
        .current_dir(root.path())
        .output()
        .expect("run xsht lint --fix");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(root.path().join("helper.xsh")).expect("read fixed helper"),
        "##! Helper module.\n## Exports a value.\nexport const value = 1\n"
    );
}

#[test]
fn lint_entry_reachability_sees_imported_module_callables() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("helper.xsh"),
        "##! Helper module.\n## Exports a value.\nexport let value = 1\n\npure unused() -> Int {\n  return 1\n}\n",
    )
    .expect("write helper module");
    fs::write(
        root.path().join("main.xsh"),
        "use helper\nprint helper.value\n",
    )
    .expect("write entry script");

    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "."])
        .current_dir(root.path())
        .output()
        .expect("run xsht lint");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr)
            .matches("lint.unused-callable")
            .count(),
        1
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

    let output = Command::new(release_bin!("xsht"))
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

    let output = Command::new(release_bin!("xsht"))
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

    let output = Command::new(release_bin!("xsht"))
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
fn check_accepts_indexed_with_error_handlers() {
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
  } else { |err|
    return Err(err)
  }
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(release_bin!("xsht"))
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
    assert!(output.stderr.is_empty(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn test_reports_lazy_default_runtime_failure_without_panicking() {
    let root = TempDir::new().expect("create temp root");
    let tests = root.path().join("tests");
    fs::create_dir(&tests).expect("create tests directory");
    fs::write(
        tests.join("lowering.xsh"),
        "pure helper(x: Int = 1 / 0) -> Int {\n  return x\n}\n\ntest test_lowering {\n  let _ = helper()\n}\n",
    )
    .expect("write test script");

    for filter in ["tests/lowering.xsh", "test_lowering"] {
        let output = Command::new(release_bin!("xsht"))
            .args(["test", filter])
            .current_dir(root.path())
            .output()
            .expect("run xsht test");

        assert_eq!(
            output.status.code(),
            Some(1),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("division by zero"), "stdout: {stdout}");
        assert!(!stdout.contains("compact.indexed-build"), "stdout: {stdout}");
    }
}

// These CLI tests assert rendered source locations across files; a native XSH
// test cannot inspect a failing `xsht check` process's diagnostics.
#[test]
fn check_attributes_imported_parse_error_to_its_source() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("main.xsh"),
        "use helper as h\nprint tui.red()\n",
    )
    .expect("write main script");
    fs::write(
        root.path().join("helper.xsh"),
        "##! Helper.\nexport let value =\n",
    )
    .expect("write imported module");

    let output = Command::new(release_bin!("xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 diagnostic");
    assert!(stderr.contains("main.xsh:1:1"), "{stderr}");
    assert!(stderr.contains("helper.xsh:2:19"), "{stderr}");
    assert!(stderr.contains("parse.expected-expression"), "{stderr}");
    assert!(!stderr.contains("<xsh-stdlib:"), "{stderr}");
}

#[test]
fn check_accepts_imported_lazy_default_with_embedded_module_loaded() {
    let root = TempDir::new().expect("create temp root");
    fs::write(
        root.path().join("main.xsh"),
        "use helper as h\nprint tui.red()\nlet value = h.scan()\n",
    )
    .expect("write main script");
    fs::write(
        root.path().join("helper.xsh"),
        "##! Helper.\n## Returns a value.\nexport pure scan(x: Int = 1 / 0) -> Int {\n  return x\n}\n",
    )
    .expect("write imported module");

    let output = Command::new(release_bin!("xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 diagnostic");
    assert!(stderr.is_empty(), "{stderr}");
    assert!(!stderr.contains("<xsh-stdlib:"), "{stderr}");
}

#[test]
fn check_reports_public_standard_call_name_at_user_source() {
    let root = TempDir::new().expect("create temp root");
    fs::write(root.path().join("main.xsh"), "print tui.red(1)\n").expect("write main script");

    let output = Command::new(release_bin!("xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 diagnostic");
    assert!(stderr.contains("check.arity"), "{stderr}");
    assert!(stderr.contains("main.xsh:1:15"), "{stderr}");
    assert!(stderr.contains("tui.red(1)"), "{stderr}");
    assert!(!stderr.contains("<xsh-stdlib:"), "{stderr}");
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

    let output = Command::new(release_bin!("xsht"))
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

// Explicit directory selection is a CLI discovery boundary: configured extra
// roots apply to the default scan, not to a directory the caller named.
#[test]
fn check_explicit_directory_does_not_expand_parent_config_includes() {
    let root = TempDir::new().expect("create temp root");
    fs::create_dir(root.path().join("project")).expect("create project directory");
    fs::create_dir(root.path().join("extra")).expect("create configured include");
    fs::write(root.path().join("xsht-config.ini"), "include = extra\n").expect("write root config");
    fs::write(root.path().join("project/main.xsh"), "let value = 1\n")
        .expect("write project script");
    fs::write(root.path().join("extra/bad.xsh"), "let value =\n").expect("write excluded script");

    let output = Command::new(release_bin!("xsht"))
        .args(["check", "project"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
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
  } else { |err|
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

    let output = Command::new(release_bin!("xsht"))
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
    assert!(!stderr.contains("compact.indexed-build"), "stderr: {stderr}");
    assert!(stderr.contains("xsht check summary:"), "stderr: {stderr}");
    assert!(
        stderr.contains("parse.expected-expression: 1"),
        "stderr: {stderr}"
    );
}

#[test]
fn check_directory_accepts_indexed_with_error_handlers() {
    let root = TempDir::new().expect("create temp root");
    let project = root.path().join("project");
    fs::create_dir_all(&project).expect("create project dir");
    fs::write(project.join("ok.xsh"), "let value = 1\n").expect("write ok script");
    fs::write(
        project.join("bad.xsh"),
        "proc main(...argv: List[Str]) [error] -> Result[Unit] {
  with value = fallible() {
    print ${value}
  } else { |err|
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

    let output = Command::new(release_bin!("xsht"))
        .args(["check", "project"])
        .current_dir(root.path())
        .output()
        .expect("run xsht check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.is_empty(), "stderr: {stderr}");
}

#[test]
fn check_top_level_user_imports_are_skippable_for_lowerability() {
    let root = TempDir::new().expect("create temp root");
    let project = root.path().join("project");
    fs::create_dir_all(project.join("pm")).expect("create module dir");
    fs::write(
        project.join("helper.xsh"),
        "##! Helper module.\n## Exposes a test value.\nexport let value = 1\n",
    )
    .expect("write helper");
    fs::write(
        project.join("pm").join("make.xsh"),
        "##! Make helper module.\n## Exposes configured jobs.\nexport let jobs = 1\n",
    )
    .expect("write pm module");
    fs::write(
        project.join("PKGBUILD-shared.xsh"),
        "##! Shared package module.\n## Exposes the package name.\nexport let pkgname = \"demo\"\n",
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

    let output = Command::new(release_bin!("xsht"))
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
fn check_accepts_lazy_default_in_main_dependency() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "pure helper(x: Int = 1 / 0) -> Int {
  return x
}

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let _ = helper()
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(release_bin!("xsht"))
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
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.is_empty(), "stderr: {stderr}");
}

#[test]
fn check_accepts_lazy_default_in_nested_top_level_call() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "pure scan_corpus(x: Int = 1 / 0) -> Int {
  return x
}

let report = {corpus: scan_corpus()}
",
    )
    .expect("write script");

    let output = Command::new(release_bin!("xsht"))
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
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.is_empty(), "stderr: {stderr}");
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

    let output = Command::new(release_bin!("xsht"))
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

    let output = Command::new(release_bin!("xsht"))
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
    let line = if stripped.starts_with(\"export \") { (stripped.split(\"export \").get(1) ?? \"\").trim() } else { stripped }
    if line.starts_with(f\"{name}=\") {
      return (line.split(\"=\").get(1) ?? \"\").trim().replace(\"\\\"\", \"\").replace(\"'\", \"\")
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

    let output = Command::new(release_bin!("xsht"))
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

    let output = Command::new(release_bin!("xsht"))
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

    let output = Command::new(release_bin!("xsht"))
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
fn check_explicit_list_annotation_validates_any_result_binding() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let stored: Record = {deps: []}
  let deps: List[Str] = stored.get(\"deps\")?.require(List[Str])?
  print \"deps\" deps.len() deps.join(\" \")
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(release_bin!("xsht"))
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
    fs::write(
        root.path().join("remote.xsh"),
        "##! Remote module.\n## Exposes the imported value.\nexport let value = 1\n",
    )
    .expect("write module");
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

    let output = Command::new(release_bin!("xsht"))
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
  let built_batches = pending |> par-map(jobs: 1) { |pkg| build_world_package(pkg) }
  for built in built_batches {
    let outcome: Result[List[BuiltPackage]] = built
    let packages = outcome?
    print ${packages.len()}
    print ${packages[0].metadata_sha256}
  }
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(release_bin!("xsht"))
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

    let output = Command::new(release_bin!("xsht"))
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
fn check_nested_par_map_enumerate_preserves_line_type() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "type Finding = {line: Int, text: Str}

proc read_source(src: Str) [error] -> Result[Str] {
  return Ok(src)
}

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let sources = [\" alpha\\n beta \"]
  let findings: List[Finding] = sources
    |> par-map { |source|
      var hits: List[Finding] = []

      match read_source(source) {
        Ok(src) => {
          for item in src.lines() |> enumerate() {
            let line_num = item.index + 1
            let line = item.value
            hits = hits.push({line: line_num, text: line.trim()})
          }
        }
        Err(_) => {}
      }

      hits
    }
    |> flat-map { |hits|
      hits
    }

  print ${findings.len()}
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(release_bin!("xsht"))
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

    let output = Command::new(release_bin!("xsht"))
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
fn check_dynamic_record_get_requires_explicit_validation() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let exports: Any = {sources: [\"a\", \"b\"]}
  let checked = exports.require(Record)?
  if \"sources\" in checked {
    let sources: List[Str] = checked.get(\"sources\")?.require(List[Str])?
    print ${sources.len()}
  }
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(release_bin!("xsht"))
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

    let output = Command::new(release_bin!("xsht"))
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
fn check_compact_lowerability_accepts_lowered_record_methods() {
    let root = TempDir::new().expect("create temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let exports: Record = {sources: {name: \"demo\"}}
  let sources = exports.get(\"sources\")?.require(Record)?
  if sources.keys().len() != 0 {
    return Ok()
  }
  return Ok()
}
",
    )
    .expect("write script");

    let output = Command::new(release_bin!("xsht"))
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
fn check_rejects_main_without_spread_parameter_but_accepts_spread() {
    let root = TempDir::new().expect("create temp root");
    let nonspread = root.path().join("nonspread.xsh");
    fs::write(
        &nonspread,
        "proc main(argv: List[Str]) [env, error] {\n  print \"hello\"\n}\n",
    )
    .expect("write script");

    let output = Command::new(release_bin!("xsht"))
        .args(["check", "nonspread.xsh"])
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
        stderr.contains("compact.main-missing-spread"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("spread form `(...argv: List[Str])`"),
        "stderr: {stderr}"
    );

    let spread = root.path().join("spread.xsh");
    fs::write(
        &spread,
        "proc main(...argv: List[Str]) [fs, env, error] {\n  print \"hello\"\n}\n",
    )
    .expect("write script");

    let output = Command::new(release_bin!("xsht"))
        .args(["check", "spread.xsh"])
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
fn lint_returns_interrupted_status_for_pending_sigint() {
    let _lock = SIGNAL_TEST_LOCK.lock().unwrap();
    let _guard = xsh::process::install_cancellation_signal_handlers()
        .expect("install cancellation signal handlers");
    xsh::process::clear_cancellation_request();
    let kill_result = unsafe { libc::kill(libc::getpid(), libc::SIGINT) };
    assert_eq!(kill_result, 0);

    let output = xsht::cli::lint_files(&["unused.xsh".to_string()], false, false, None);
    xsh::process::clear_cancellation_request();

    assert_eq!(output.status, 130);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("interrupted by SIGINT")
    );
}

#[test]
fn membership_migration_after_removal_fixes_shared_import_once_and_is_idempotent() {
    let root = TempDir::new().expect("create migration project");
    let helper = root.path().join("helper.xsh");
    fs::write(&helper, "##! Membership fixture.\n## Tests membership.\nexport pure present(text: Str) -> Bool { return text.contains(\"needle\") }\n").unwrap();
    for entry in ["first.xsh", "second.xsh"] {
        fs::write(root.path().join(entry), "use helper\nassert helper.present(\"needle\")\n").unwrap();
    }
    let removed = Command::new(release_bin!("xsht")).args(["check", "first.xsh"]).current_dir(root.path()).output().unwrap();
    assert_eq!(removed.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&removed.stderr).contains("check.removed-membership"));
    let fixed = Command::new(release_bin!("xsht")).args(["lint", "--fix", "first.xsh", "second.xsh"]).current_dir(root.path()).output().unwrap();
    assert!(fixed.status.success(), "{}", String::from_utf8_lossy(&fixed.stderr));
    let text = fs::read_to_string(&helper).unwrap();
    assert!(text.contains("\"needle\" in text"), "{text}");
    let second = Command::new(release_bin!("xsht")).args(["lint", "--fix", "first.xsh", "second.xsh"]).current_dir(root.path()).output().unwrap();
    assert!(second.status.success(), "{}", String::from_utf8_lossy(&second.stderr));
    assert_eq!(fs::read_to_string(helper).unwrap(), text);
}

#[test]
fn membership_migration_does_not_suppress_unrelated_checker_failure() {
    let root = TempDir::new().expect("create migration project");
    let script = root.path().join("main.xsh");
    fs::write(&script, "let result: Int = \"abc\".contains(\"a\")\nlet count: Int = \"invalid\"\n").unwrap();
    let output = Command::new(release_bin!("xsht")).args(["lint", "--fix", "main.xsh"]).current_dir(root.path()).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("check.type-mismatch"));
}

#[test]
fn check_rejects_unreachable_invalid_regex_literals_without_execution() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("invalid.xsh"), "print \"must not execute\"\npure unused() -> Regex { rx\"(\" }\n").unwrap();
    let output = Command::new(release_bin!("xsht")).args(["check", "invalid.xsh"]).current_dir(root.path()).output().unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("must not execute"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("check.regex-literal"), "{stderr}");
    assert!(stderr.contains("invalid.xsh:2:"), "{stderr}");
}

#[test]
fn regex_literal_lint_fixture_preserves_execution_and_is_idempotent() {
    let root = TempDir::new().unwrap();
    let fixture = root.path().join("regex.xsh");
    fs::write(&fixture, "let assignment = regex.compile(\"^([A-Z]+)=([0-9]+)$\")? # keep\nprint ${assignment.matches(\"COUNT=42\")} ${assignment.captures(\"COUNT=42\")[2]} ${assignment.replace(\"COUNT=42\", \"$2\")}\n").unwrap();
    let run = |arguments: &[&str]| Command::new(release_bin!("xsht")).args(arguments).current_dir(root.path()).output().unwrap();
    let before = run(&["trace", "regex.xsh"]);
    assert!(before.status.success(), "{}", String::from_utf8_lossy(&before.stderr));
    let first = run(&["lint", "--fix", "regex.xsh"]);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    let fixed = fs::read_to_string(&fixture).unwrap();
    assert!(fixed.contains("rx\"^([A-Z]+)=([0-9]+)$\" # keep"), "{fixed}");
    let second = run(&["lint", "--fix", "regex.xsh"]);
    assert!(second.status.success(), "{}", String::from_utf8_lossy(&second.stderr));
    assert_eq!(fixed, fs::read_to_string(&fixture).unwrap());
    let after = run(&["trace", "regex.xsh"]);
    assert!(after.status.success(), "{}", String::from_utf8_lossy(&after.stderr));
    assert_eq!(before.stdout, after.stdout);
}

#[test]
fn check_validates_regex_literals_in_unused_imported_functions() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.xsh"), "use broken\nprint \"must not execute\"\n").unwrap();
    fs::write(root.path().join("broken.xsh"), "pure never_called() -> Regex { rx\"[\" }\n").unwrap();
    let output = Command::new(release_bin!("xsht")).args(["check", "main.xsh"]).current_dir(root.path()).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("check.regex-literal"), "{stderr}");
    assert!(stderr.contains("broken.xsh:1:"), "{stderr}");
}

#[test]
fn private_pure_return_annotation_mode_and_lint_policy_preserve_each_other() {
    let root = TempDir::new().expect("temp root");
    let script = root.path().join("main.xsh");
    fs::write(&script, "pure label(name: Str) { name.trim() }\nprint label(\"ready\")\n").unwrap();
    fs::write(root.path().join("xsht-config.ini"), "[check]\nannotate = returns\n[lint]\nprefer-inferred-pure-returns = true\n").unwrap();
    let output = Command::new(release_bin!("xsht")).args(["check", "--annotate", "main.xsh"]).current_dir(root.path()).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let annotated = fs::read_to_string(&script).unwrap();
    assert!(annotated.contains("-> Str"), "{annotated}");
    let output = Command::new(release_bin!("xsht")).args(["lint", "--fix", "main.xsh"]).current_dir(root.path()).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(fs::read_to_string(&script).unwrap(), annotated);
}

#[test]
fn private_pure_return_annotation_removal_cli_is_opt_in_and_idempotent() {
    let root = TempDir::new().expect("temp root");
    let script = root.path().join("main.xsh");
    let source = "pure label(name: Str) -> Str { name.trim() }\nprint label(\"ready\")\n";
    fs::write(&script, source).unwrap();
    let run = || Command::new(release_bin!("xsht")).args(["lint", "--fix", "main.xsh"]).current_dir(root.path()).output().unwrap();
    assert!(run().status.success());
    assert_eq!(fs::read_to_string(&script).unwrap(), source);
    fs::write(root.path().join("xsht-config.ini"), "[lint]\nprefer-inferred-pure-returns = true\n").unwrap();
    let output = run();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let fixed = fs::read_to_string(&script).unwrap();
    assert!(!fixed.contains("-> Str"), "{fixed}");
    assert!(run().status.success());
    assert_eq!(fs::read_to_string(&script).unwrap(), fixed);
}

#[test]
fn value_pipeline_hole_lint_cli_converges_without_changing_execution() {
    let root = TempDir::new().unwrap();
    let script = root.path().join("main.xsh");
    fs::write(&script, "pure first(value: Int) -> Int { value + 1 }\npure second(prefix: Int, value: Int) -> Int { prefix + value }\npure third(value: Int) -> Int { value * 2 }\nlet initial = first(2)\nlet next = second(10, value: initial)\nlet final_value = third(next)\nprint $final_value\n").unwrap();
    let run = |arguments: &[&str]| Command::new(release_bin!("xsht")).args(arguments).current_dir(root.path()).output().unwrap();
    let before = run(&["trace", "main.xsh"]);
    assert!(before.status.success(), "{}", String::from_utf8_lossy(&before.stderr));
    let output = run(&["lint", "--fix", "main.xsh"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let fixed = fs::read_to_string(&script).unwrap();
    assert!(fixed.contains("first(2) |> second(10, value: _) |> third(_)"), "{fixed}");
    let output = run(&["lint", "--fix", "main.xsh"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(fixed, fs::read_to_string(&script).unwrap());
    let after = run(&["trace", "main.xsh"]);
    assert!(after.status.success(), "{}", String::from_utf8_lossy(&after.stderr));
    assert_eq!(before.stdout, after.stdout);
}

fn processes_with_marker(marker: &str) -> Vec<String> {
    let listing = Command::new("ps").args(["-A", "-o", "pid=,args="]).output().expect("list processes");
    String::from_utf8_lossy(&listing.stdout).lines().filter(|line| line.contains(marker)).map(str::to_owned).collect()
}

#[test]
fn test_runner_cancellation_stops_run_script_descendants() {
    for signal in [libc::SIGTERM, libc::SIGINT] {
        let root = TempDir::new().expect("temporary descendant fixture");
        let marker = format!("xsht-descendant-{}-{signal}", std::process::id());
        fs::create_dir(root.path().join("tests")).expect("create test root");
        fs::write(root.path().join("tests/spawn.xsh"), format!("\
test spawns_descendants {{ |ctx|
  let output = test.run_script(ctx, \"\"\"
run sh -c \"sleep 300; : {marker}-grandchild\"
\"\"\", [\"{marker}-child\"])?
  assert output.success
}}
")).expect("write descendant fixture");
        let mut runner = Command::new(release_bin!("xsht"))
            .args(["test", "--jobs", "1"])
            .current_dir(root.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("start xsht test");
        let started = std::time::Instant::now();
        loop {
            let found = processes_with_marker(&marker);
            if found.iter().any(|line| line.contains("-child")) && found.iter().any(|line| line.contains("-grandchild")) {
                break;
            }
            if started.elapsed() > std::time::Duration::from_secs(120) || runner.try_wait().expect("poll runner").is_some() {
                let _ = runner.kill();
                panic!("descendants never started: {found:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(unsafe { libc::kill(runner.id() as libc::pid_t, signal) }, 0);
        let status = runner.wait().expect("wait for runner");
        assert_eq!(std::os::unix::process::ExitStatusExt::signal(&status), None, "{status:?}");
        assert_eq!(status.code(), Some(128 + signal));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !processes_with_marker(&marker).is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(processes_with_marker(&marker), Vec::<String>::new());
    }
}

#[test]
fn test_runner_times_out_hung_tests_and_stops_their_descendants() {
    let root = TempDir::new().expect("temporary timeout fixture");
    let marker = format!("xsht-timeout-{}", std::process::id());
    fs::create_dir(root.path().join("tests")).expect("create test root");
    fs::write(root.path().join("tests/hang.xsh"), format!("\
test spins {{ |ctx|
  defer {{ print \"spins cleanup ran\" }}
  var n = 0
  while true {{ n += 1 }}
}}

test sleeping_script {{ |ctx|
  let output = test.run_script(ctx, \"\"\"
run sh -c \"sleep 300; : {marker}-grandchild\"
\"\"\", [\"{marker}-child\"])?
  assert output.success
}}

test sleeping_command {{ |ctx|
  run sh -c \"sleep 300; : {marker}-command\"
}}

test own_shorter_limit {{ |ctx|
  test.timeout(ctx, 200ms)
  time.sleep(30s)
}}

test own_longer_limit {{ |ctx|
  test.timeout(ctx, 30s)
  time.sleep(1500ms)
}}

test passes {{ |ctx|
  assert 1 == 1
}}
")).expect("write timeout fixture");
    let started = std::time::Instant::now();
    let mut runner = Command::new(release_bin!("xsht"))
        .args(["test", "--jobs", "2", "--timeout", "1s"])
        .current_dir(root.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("start xsht test");
    let status = loop {
        if let Some(status) = runner.try_wait().expect("poll runner") {
            break status;
        }
        if started.elapsed() > std::time::Duration::from_secs(60) {
            let _ = runner.kill();
            panic!("hung tests did not time out");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let mut stdout = String::new();
    std::io::Read::read_to_string(&mut runner.stdout.take().expect("runner stdout"), &mut stdout)
        .expect("read runner stdout");
    assert!(started.elapsed() < std::time::Duration::from_secs(30), "{stdout}");
    assert_eq!(status.code(), Some(1), "{stdout}");
    for name in ["spins", "sleeping_script", "sleeping_command", "own_shorter_limit"] {
        assert!(stdout.contains(&format!("tests/hang.xsh::{name} ... TIMEOUT")), "{name}: {stdout}");
    }
    assert!(stdout.contains("---- tests/hang.xsh::spins ----\nstdout:\nspins cleanup ran\nTIMEOUT after 1s\n"), "{stdout}");
    assert!(stdout.contains("TIMEOUT after 200ms"), "{stdout}");
    assert!(stdout.contains("tests/hang.xsh::own_longer_limit ... ok"), "{stdout}");
    assert!(stdout.contains("tests/hang.xsh::passes ... ok"), "{stdout}");
    assert!(stdout.contains("test result: FAILED. 2 passed; 4 failed; 0 skipped"), "{stdout}");
    assert_eq!(processes_with_marker(&marker), Vec::<String>::new());

    let invalid = Command::new(release_bin!("xsht"))
        .args(["test", "--timeout", "soon"])
        .current_dir(root.path())
        .output()
        .expect("run xsht test with an invalid timeout");
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("`--timeout` expects a duration"));
}

#[test]
fn native_test_declaration_discovery_preserves_names_and_runs_each_once() {
    let root = TempDir::new().expect("temporary native declaration fixture");
    fs::create_dir(root.path().join("tests")).expect("create test root");
    fs::write(root.path().join("tests/explicit.xsh"), "\
pure helper() -> Int { 2 }
proc ordinary_helper() { print helper() }
proc test_named_helper(value: Int) -> Int { value }
test test_old_name { assert test_named_helper(helper()) == 2 }
test no_prefix { |ctx| assert \"no_prefix\" in ctx.name }
test discarded { |_| }
").expect("write native declaration fixture");
    let list = Command::new(release_bin!("xsht"))
        .args(["test", "--list"]).current_dir(root.path()).output().expect("list declarations");
    assert!(list.status.success());
    assert_eq!(String::from_utf8(list.stdout).unwrap(), "tests/explicit.xsh::discarded\ntests/explicit.xsh::no_prefix\ntests/explicit.xsh::test_old_name\n");
    let run = Command::new(release_bin!("xsht"))
        .args(["test", "--jobs", "1"]).current_dir(root.path()).output().expect("run declarations");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stdout));
    assert!(String::from_utf8_lossy(&run.stdout).contains("3 passed; 0 failed"));
}

#[test]
fn native_test_declaration_legacy_proc_has_actionable_failure() {
    let root = TempDir::new().expect("temporary legacy fixture");
    fs::create_dir(root.path().join("tests")).expect("create test root");
    fs::write(root.path().join("tests/legacy.xsh"), "proc test_old(ctx: TestContext) -> Result[Unit] {}\n")
        .expect("write legacy fixture");
    let run = Command::new(release_bin!("xsht"))
        .args(["test", "--jobs", "1"]).current_dir(root.path()).output().expect("run legacy fixture");
    assert_eq!(run.status.code(), Some(1));
    let output = String::from_utf8_lossy(&run.stdout);
    assert!(output.contains("check.legacy-test-proc"), "{output}");
    assert!(output.contains("keep the exact declared name"), "{output}");
    assert!(!output.contains("0 passed; 0 failed"), "{output}");
    let filtered = Command::new(release_bin!("xsht"))
        .args(["test", "--exact", "tests/legacy.xsh::test_old"])
        .current_dir(root.path()).output().expect("run exact legacy filter");
    assert_eq!(filtered.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&filtered.stdout).contains("check.legacy-test-proc"));
}

#[test]
fn native_test_declaration_import_registers_without_execution_or_discovery() {
    let root = TempDir::new().expect("temporary imported declaration fixture");
    fs::create_dir(root.path().join("tests")).expect("create test root");
    fs::write(root.path().join("tests/helper.xsh"), "##! Import registration fixture.\n## Returns the fixture value.\nexport pure value() -> Int { 7 }\ntest imported { assert false }\n")
        .expect("write imported module");
    fs::write(root.path().join("tests/entry.xsh"), "use helper\ntest entry { assert helper.value() == 7 }\n")
        .expect("write entry fixture");
    let run = Command::new(release_bin!("xsht"))
        .args(["test", "--jobs", "1", "tests/entry.xsh"]).current_dir(root.path()).output().expect("run imported fixture");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stdout));
    let output = String::from_utf8_lossy(&run.stdout);
    assert!(output.contains("1 passed; 0 failed"), "{output}");
    assert!(!output.contains("::imported"), "{output}");
}

#[test]
fn native_test_declaration_duplicate_and_callable_collision_are_rejected() {
    let root = TempDir::new().expect("temporary colliding declaration fixture");
    let source = root.path().join("collision.xsh");
    for text in ["test same {}\ntest same {}", "pure same() -> Int { 1 }\ntest same {}", "let same = 1\ntest same {}", "test same {}\nlet {same} = {same: 1}", "use env as same\ntest same {}"] {
        fs::write(&source, text).expect("write collision fixture");
        let output = Command::new(release_bin!("xsht"))
            .arg("check").arg(&source).output().expect("check collision");
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("check.duplicate-name"));
    }
    fs::write(&source, "use helper\ntest entry {}\n").expect("write importing collision fixture");
    for text in ["test same {}\ntest same {}", "test same {}\npure same() -> Int { 1 }", "let same = 1\ntest same {}"] {
        fs::write(root.path().join("helper.xsh"), text).expect("write module collision fixture");
        let output = Command::new(release_bin!("xsht"))
            .arg("check").arg(&source).output().expect("check module collision");
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("check.duplicate-name"));
    }
}

#[test]
fn enum_migration_fix_preserves_comments_aliases_and_imports() {
    let root = TempDir::new().expect("enum migration fixture");
    let module = root.path().join("choice.xsh");
    fs::write(&module, "##! Nominal choices.\n## A choice.\nexport type Choice =\n  Selected(Int) # selected café\n  | Empty # absent\n## Same nominal identity.\nexport type Alias = Choice\n").expect("write legacy enum module");
    let entry = root.path().join("entry.xsh");
    fs::write(&entry, "use choice as c\nlet value: c.Alias = c.Selected(7)\nmatch value { c.Selected(number) => print $number; c.Empty => print \"empty\" }\n").expect("write enum entry");
    let run = |arguments: &[&str]| Command::new(release_bin!("xsht"))
        .args(arguments).current_dir(root.path()).output().expect("run enum fixture");
    let rejected = run(&["check", "entry.xsh"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("parse.enum-migration"));
    let first = run(&["lint", "--fix", "entry.xsh"]);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    let fixed = fs::read_to_string(&module).expect("read migrated module");
    for fragment in ["export enum Choice {", "# selected café", "# absent", "export type Alias = Choice"] {
        assert!(fixed.contains(fragment), "{fixed}");
    }
    let second = run(&["lint", "--fix", "entry.xsh"]);
    assert!(second.status.success(), "{}", String::from_utf8_lossy(&second.stderr));
    assert_eq!(fixed, fs::read_to_string(&module).expect("read stable module"));
    let checked = run(&["check", "entry.xsh"]);
    assert!(checked.status.success(), "{}", String::from_utf8_lossy(&checked.stderr));
}

#[test]
fn enum_migration_fix_retains_unrelated_checker_errors() {
    let root = TempDir::new().expect("enum rejected migration fixture");
    let entry = root.path().join("entry.xsh");
    let source = "type Choice = Selected(Int) | Empty\nlet value = Selected(\"wrong\")\n";
    fs::write(&entry, source).expect("write invalid enum use");
    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "--fix", "entry.xsh"]).current_dir(root.path()).output().expect("check migration candidate");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("check.type-mismatch"), "{stderr}");
    assert_eq!(source, fs::read_to_string(&entry).expect("read unchanged invalid source"));
}

#[test]
fn signature_cli_safe_fix_preserves_process_results_and_is_idempotent() {
    let root = TempDir::new().unwrap();
    let fixture = root.path().join("entry.xsh");
    fs::write(&fixture, "type Options = {jobs: Int, verbose: Bool}\nproc main(...argv: List[Str]) [error] {\n  let {jobs, verbose}: Options = cli.parse(argv, {jobs: {kind: \"Int\", default: 4, help: \"Int, default: 4\"}, verbose: {kind: \"Bool\", default: false, help: \"Bool, default: false\"}})?\n  let shown = Options(jobs:, verbose:)\n  print $shown.jobs $shown.verbose\n}\n").unwrap();
    let run = |arguments: &[&str]| Command::new(release_bin!("xsht")).args(arguments)
        .current_dir(root.path()).output().unwrap();
    let cases = [vec![], vec!["--jobs=8", "--verbose"], vec!["--help"], vec!["--jobs=invalid"], vec!["--jobs=2", "--jobs=3"]];
    let invoke = |arguments: &Vec<&str>| {
        let mut command = vec!["trace", "entry.xsh", "--"];
        command.extend(arguments.iter().copied());
        let output = run(&command);
        (output.status.code(), output.stdout)
    };
    let before = cases.iter().map(invoke).collect::<Vec<_>>();
    let first = run(&["lint", "--fix", "entry.xsh"]);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    let fixed = fs::read_to_string(&fixture).unwrap();
    assert!(fixed.contains("cli main("), "{fixed}");
    let second = run(&["lint", "--fix", "entry.xsh"]);
    assert!(second.status.success(), "{}", String::from_utf8_lossy(&second.stderr));
    assert_eq!(fixed, fs::read_to_string(&fixture).unwrap());
    assert_eq!(before, cases.iter().map(invoke).collect::<Vec<_>>());
}

#[test]
fn check_imported_error_annotation_retains_constructor_identity() {
    let root = TempDir::new().expect("create nominal import fixture");
    fs::write(root.path().join("helper.xsh"),
        "##! Error identity fixture.\n## An exported failure family.\nexport error HelperError = Failed(detail: Str) : Temporary\n")
        .expect("write exported family");
    fs::write(root.path().join("main.xsh"),
        "use helper\nlet failure: helper.HelperError = helper.HelperError.Failed(detail: \"failed\")\n")
        .expect("write qualified annotation");
    let output = Command::new(release_bin!("xsht"))
        .args(["check", "main.xsh"])
        .current_dir(root.path())
        .output()
        .expect("check imported nominal identity");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
