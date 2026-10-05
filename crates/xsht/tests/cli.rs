#![allow(clippy::single_call_fn)]

use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn mixed_enum_and_record_require_migration_rechecks_import_graph_and_converges_in_stages() {
    let root = TempDir::new().expect("mixed migration fixture");
    let entry = root.path().join("entry.xsh");
    let module = root.path().join("choice.xsh");
    fs::write(&module, "##! Choices.\n## A nominal choice.\nexport type Choice = Selected(Int) | Empty # retained café\n").unwrap();
    fs::write(&entry, "use choice as c\n## A name.\nexport type Name = {name: Str}\nlet _ = record.require({name: \"café\"}, {name: \"Str\"})? # retained receiver\nlet choice: c.Choice = c.Selected(7)\nprint \"café\"\nmatch choice { c.Selected(number) => print $number; c.Empty => print \"empty\" }\n").unwrap();
    let run = |arguments: &[&str]| {
        Command::new(release_bin!("xsht"))
            .args(arguments)
            .current_dir(root.path())
            .output()
            .unwrap()
    };
    let before = run(&["check", "entry.xsh"]);
    assert!(!before.status.success());
    let first = run(&["lint", "--fix", "entry.xsh"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let fixed_entry = fs::read_to_string(&entry).unwrap();
    let fixed_module = fs::read_to_string(&module).unwrap();
    assert!(
        fixed_entry.contains("{name: \"café\"}.require(Name)? # retained receiver"),
        "{fixed_entry}"
    );
    assert!(
        fixed_module.contains("export enum Choice {"),
        "{fixed_module}"
    );
    assert!(fixed_module.contains("# retained café"), "{fixed_module}");
    let checked = run(&["check", "entry.xsh"]);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let executed = run(&["trace", "entry.xsh"]);
    assert!(
        executed.status.success(),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&executed.stdout), "café\n7\n");
    // Exact syntax/API repair makes ordinary lints available on the next pass;
    // they can then remove identity schema validation and normalize layout.
    let second = run(&["lint", "--fix", "entry.xsh"]);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let canonical_entry = fs::read_to_string(&entry).unwrap();
    let canonical_module = fs::read_to_string(&module).unwrap();
    assert!(
        canonical_entry.contains("# retained receiver"),
        "{canonical_entry}"
    );
    assert!(
        canonical_module.contains("# retained café"),
        "{canonical_module}"
    );
    let after = run(&["trace", "entry.xsh"]);
    assert!(
        after.status.success(),
        "{}",
        String::from_utf8_lossy(&after.stderr)
    );
    assert_eq!(executed.stdout, after.stdout);
    let third = run(&["lint", "--fix", "entry.xsh"]);
    assert!(
        third.status.success(),
        "{}",
        String::from_utf8_lossy(&third.stderr)
    );
    assert_eq!(canonical_entry, fs::read_to_string(&entry).unwrap());
    assert_eq!(canonical_module, fs::read_to_string(&module).unwrap());
}

#[test]
fn mixed_enum_and_record_require_migration_refuses_unproved_identity() {
    let root = TempDir::new().expect("unproved mixed migration fixture");
    let entry = root.path().join("entry.xsh");
    let module = root.path().join("choice.xsh");
    let module_source =
        "##! Choices.\n## A nominal choice.\nexport type Choice = Selected(Int) | Empty\n";
    let entry_source = "use choice as c\ntype Name = {name: Str}\nlet _ = record.require({name: 7}, {name: \"Str\"})?\nlet choice: c.Choice = c.Selected(7)\n";
    fs::write(&entry, entry_source).unwrap();
    fs::write(&module, module_source).unwrap();
    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "--fix", "entry.xsh"])
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let diagnostic_text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        diagnostic_text.contains("lint.removed-record-require"),
        "{diagnostic_text}"
    );
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
            if broken_module {
                module_source.push_str("let broken: Int = \"wrong\"\n");
            } else {
                entry_source.push_str("let broken: Int = \"wrong\"\n");
            }
            "check.type-mismatch"
        };
        fs::write(&entry, &entry_source).unwrap();
        fs::write(&module, &module_source).unwrap();
        let output = Command::new(release_bin!("xsht"))
            .args(["lint", "--fix", "entry.xsh"])
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected_code),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(entry_source, fs::read_to_string(&entry).unwrap());
        assert_eq!(module_source, fs::read_to_string(&module).unwrap());
    }
}

#[test]
fn removed_record_require_cli_fix_rechecks_and_converges_in_stages() {
    let root = TempDir::new().expect("record migration fixture");
    let entry = root.path().join("entry.xsh");
    fs::write(&entry, "export type Name = {name: Str}\nconst required = {name: \"Str\"}\nlet value = record.require({name: \"café\", extra: 7}, required)?\nprint $value.name\n").unwrap();
    let run = |arguments: &[&str]| {
        Command::new(release_bin!("xsht"))
            .args(arguments)
            .current_dir(root.path())
            .output()
            .unwrap()
    };
    let before = run(&["check", "entry.xsh"]);
    assert_eq!(before.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&before.stderr).contains("check.removed-record-require"));
    let first = run(&["lint", "--fix", "entry.xsh"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let fixed = fs::read_to_string(&entry).unwrap();
    assert!(!fixed.contains("record.require"), "{fixed}");
    assert!(fixed.contains("café") || fixed.contains("caf\\u{e9}"));
    let after = run(&["check", "entry.xsh"]);
    assert!(
        after.status.success(),
        "{}",
        String::from_utf8_lossy(&after.stderr)
    );
    let before_ordinary_fixes = run(&["trace", "entry.xsh"]);
    assert!(
        before_ordinary_fixes.status.success(),
        "{}",
        String::from_utf8_lossy(&before_ordinary_fixes.stderr)
    );
    let second = run(&["lint", "--fix", "entry.xsh"]);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let canonical = fs::read_to_string(&entry).unwrap();
    let third = run(&["lint", "--fix", "entry.xsh"]);
    assert!(
        third.status.success(),
        "{}",
        String::from_utf8_lossy(&third.stderr)
    );
    assert_eq!(canonical, fs::read_to_string(&entry).unwrap());
    let executed = run(&["trace", "entry.xsh"]);
    assert!(
        executed.status.success(),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
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
            .args(["lint", "--fix", "entry.xsh"])
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(source, fs::read_to_string(&entry).unwrap());
    }
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
        "check",
        "fmt",
        "lint",
        "ast",
        "highlight",
        "desugar",
        "grammar",
        "trace",
        "api",
        "test",
        "grep",
        "refactor",
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
fn xsht_grammar_prints_the_productions() {
    let ebnf = Command::new(release_bin!("xsht"))
        .arg("grammar")
        .output()
        .expect("run xsht grammar");
    assert!(ebnf.status.success());
    let stdout = String::from_utf8(ebnf.stdout).unwrap();
    assert!(stdout.contains("(* Expressions *)"), "{stdout}");
    assert!(stdout.contains("\nprogram = "), "{stdout}");

    let json = Command::new(release_bin!("xsht"))
        .args(["grammar", "--format", "json"])
        .output()
        .expect("run xsht grammar");
    assert!(json.status.success());
    assert!(
        String::from_utf8(json.stdout)
            .unwrap()
            .starts_with("{\"sections\":[")
    );

    let invalid = Command::new(release_bin!("xsht"))
        .args(["grammar", "--format", "yaml"])
        .output()
        .expect("run xsht grammar");
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8(invalid.stderr)
            .unwrap()
            .contains("must be ebnf or json")
    );
}

#[test]
fn xsht_highlight_prints_runs_that_rebuild_the_source() {
    let dir = TempDir::new().expect("temporary highlight fixture");
    let script = dir.path().join("sample.xsh");
    let source = "# note\nlet n = f\"{n:>4} \\u{41}\" ?? null\n";
    std::fs::write(&script, source).unwrap();

    let output = Command::new(release_bin!("xsht"))
        .arg("highlight")
        .arg(&script)
        .output()
        .expect("run xsht highlight");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "{\"kind\":\"comment\",\"text\":\"# note\"}");
    assert_eq!(lines[1], "{\"kind\":\"plain\",\"text\":\"\\n\"}");
    assert_eq!(lines[2], "{\"kind\":\"keyword\",\"text\":\"let\"}");
    assert!(
        lines.contains(&"{\"kind\":\"interpolation\",\"text\":\":>4}\"}"),
        "{stdout}"
    );
    assert!(
        lines.contains(&"{\"kind\":\"constant\",\"text\":\"null\"}"),
        "{stdout}"
    );
}

#[test]
fn xsht_highlight_reports_bad_arguments_and_unreadable_files() {
    let dir = TempDir::new().expect("temporary highlight error fixture");
    let binary = dir.path().join("binary.xsh");
    std::fs::write(&binary, [0xff, 0xfe]).unwrap();

    for (args, message) in [
        (vec!["highlight".to_string()], "requires SCRIPT"),
        (
            vec!["highlight".to_string(), "a.xsh".to_string(), "b.xsh".to_string()],
            "exactly one SCRIPT",
        ),
        (
            vec!["highlight".to_string(), "missing.xsh".to_string()],
            "failed to read 'missing.xsh'",
        ),
        (
            vec!["highlight".to_string(), binary.display().to_string()],
            "failed to read",
        ),
    ] {
        let output = Command::new(release_bin!("xsht"))
            .args(&args)
            .current_dir(dir.path())
            .output()
            .expect("run xsht highlight");
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains(message), "{args:?}: {stderr}");
    }
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
        // `check` and `lint` close stderr with their timing line; `fmt` prints nothing.
        let stderr = match args[0] {
            "fmt" => std::str::from_utf8(&output.stderr).expect("UTF-8 stderr"),
            command => crate::stderr_before_timing_line(command, &output.stderr),
        };
        assert!(stderr.is_empty(), "{}: {stderr}", args.join(" "));
    }
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
        assert!(
            !stdout.contains("compact.indexed-build"),
            "stdout: {stdout}"
        );
    }
}

#[test]
fn membership_migration_after_removal_fixes_shared_import_once_and_is_idempotent() {
    let root = TempDir::new().expect("create migration project");
    let helper = root.path().join("helper.xsh");
    fs::write(&helper, "##! Membership fixture.\n## Tests membership.\nexport pure present(text: Str) -> Bool { return text.contains(\"needle\") }\n").unwrap();
    for entry in ["first.xsh", "second.xsh"] {
        fs::write(
            root.path().join(entry),
            "use helper\nassert helper.present(\"needle\")\n",
        )
        .unwrap();
    }
    let removed = Command::new(release_bin!("xsht"))
        .args(["check", "first.xsh"])
        .current_dir(root.path())
        .output()
        .unwrap();
    assert_eq!(removed.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&removed.stderr).contains("check.removed-membership"));
    let fixed = Command::new(release_bin!("xsht"))
        .args(["lint", "--fix", "first.xsh", "second.xsh"])
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(
        fixed.status.success(),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    let text = fs::read_to_string(&helper).unwrap();
    assert!(text.contains("\"needle\" in text"), "{text}");
    let second = Command::new(release_bin!("xsht"))
        .args(["lint", "--fix", "first.xsh", "second.xsh"])
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(fs::read_to_string(helper).unwrap(), text);
}

#[test]
fn membership_migration_does_not_suppress_unrelated_checker_failure() {
    let root = TempDir::new().expect("create migration project");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "let result: Int = \"abc\".contains(\"a\")\nlet count: Int = \"invalid\"\n",
    )
    .unwrap();
    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "--fix", "main.xsh"])
        .current_dir(root.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("check.type-mismatch"));
}

#[test]
fn regex_literal_lint_fixture_preserves_execution_and_is_idempotent() {
    let root = TempDir::new().unwrap();
    let fixture = root.path().join("regex.xsh");
    fs::write(&fixture, "let assignment = regex.compile(\"^([A-Z]+)=([0-9]+)$\")? # keep\nprint ${assignment.matches(\"COUNT=42\")} ${assignment.captures(\"COUNT=42\")[2]} ${assignment.replace(\"COUNT=42\", \"$2\")}\n").unwrap();
    let run = |arguments: &[&str]| {
        Command::new(release_bin!("xsht"))
            .args(arguments)
            .current_dir(root.path())
            .output()
            .unwrap()
    };
    let before = run(&["trace", "regex.xsh"]);
    assert!(
        before.status.success(),
        "{}",
        String::from_utf8_lossy(&before.stderr)
    );
    let first = run(&["lint", "--fix", "regex.xsh"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let fixed = fs::read_to_string(&fixture).unwrap();
    assert!(
        fixed.contains("rx\"^([A-Z]+)=([0-9]+)$\" # keep"),
        "{fixed}"
    );
    let second = run(&["lint", "--fix", "regex.xsh"]);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(fixed, fs::read_to_string(&fixture).unwrap());
    let after = run(&["trace", "regex.xsh"]);
    assert!(
        after.status.success(),
        "{}",
        String::from_utf8_lossy(&after.stderr)
    );
    assert_eq!(before.stdout, after.stdout);
}

#[test]
fn private_pure_return_annotation_mode_and_lint_policy_preserve_each_other() {
    let root = TempDir::new().expect("temp root");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "pure label(name: Str) { name.trim() }\nprint label(\"ready\")\n",
    )
    .unwrap();
    fs::write(
        root.path().join("xsht-config.ini"),
        "[check]\nannotate = returns\n[lint]\nprefer-inferred-pure-returns = true\n",
    )
    .unwrap();
    let output = Command::new(release_bin!("xsht"))
        .args(["check", "--annotate", "main.xsh"])
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let annotated = fs::read_to_string(&script).unwrap();
    assert!(annotated.contains("-> Str"), "{annotated}");
    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "--fix", "main.xsh"])
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(&script).unwrap(), annotated);
}

#[test]
fn private_pure_return_annotation_removal_cli_is_opt_in_and_idempotent() {
    let root = TempDir::new().expect("temp root");
    let script = root.path().join("main.xsh");
    let source = "pure label(name: Str) -> Str { name.trim() }\nprint label(\"ready\")\n";
    fs::write(&script, source).unwrap();
    let run = || {
        Command::new(release_bin!("xsht"))
            .args(["lint", "--fix", "main.xsh"])
            .current_dir(root.path())
            .output()
            .unwrap()
    };
    assert!(run().status.success());
    assert_eq!(fs::read_to_string(&script).unwrap(), source);
    fs::write(
        root.path().join("xsht-config.ini"),
        "[lint]\nprefer-inferred-pure-returns = true\n",
    )
    .unwrap();
    let output = run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
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
    let run = |arguments: &[&str]| {
        Command::new(release_bin!("xsht"))
            .args(arguments)
            .current_dir(root.path())
            .output()
            .unwrap()
    };
    let before = run(&["trace", "main.xsh"]);
    assert!(
        before.status.success(),
        "{}",
        String::from_utf8_lossy(&before.stderr)
    );
    let output = run(&["lint", "--fix", "main.xsh"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixed = fs::read_to_string(&script).unwrap();
    assert!(
        fixed.contains("first(2) |> second(10, value: _) |> third(_)"),
        "{fixed}"
    );
    let output = run(&["lint", "--fix", "main.xsh"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fixed, fs::read_to_string(&script).unwrap());
    let after = run(&["trace", "main.xsh"]);
    assert!(
        after.status.success(),
        "{}",
        String::from_utf8_lossy(&after.stderr)
    );
    assert_eq!(before.stdout, after.stdout);
}

fn processes_with_marker(marker: &str) -> Vec<String> {
    let listing = Command::new("ps")
        .args(["-A", "-o", "pid=,args="])
        .output()
        .expect("list processes");
    String::from_utf8_lossy(&listing.stdout)
        .lines()
        .filter(|line| line.contains(marker))
        .map(str::to_owned)
        .collect()
}

#[test]
fn test_runner_cancellation_stops_run_script_descendants() {
    for signal in [libc::SIGTERM, libc::SIGINT] {
        let root = TempDir::new().expect("temporary descendant fixture");
        let marker = format!("xsht-descendant-{}-{signal}", std::process::id());
        fs::create_dir(root.path().join("tests")).expect("create test root");
        fs::write(
            root.path().join("tests/spawn.xsh"),
            format!(
                "\
test spawns_descendants {{ |ctx|
  let output = test.run_script(ctx, \"\"\"
run sh -c \"sleep 300; : {marker}-grandchild\"
\"\"\", [\"{marker}-child\"])?
  assert output.success
}}
"
            ),
        )
        .expect("write descendant fixture");
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
            if found.iter().any(|line| line.contains("-child"))
                && found.iter().any(|line| line.contains("-grandchild"))
            {
                break;
            }
            if started.elapsed() > std::time::Duration::from_secs(120)
                || runner.try_wait().expect("poll runner").is_some()
            {
                let _ = runner.kill();
                panic!("descendants never started: {found:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(unsafe { libc::kill(runner.id() as libc::pid_t, signal) }, 0);
        let status = runner.wait().expect("wait for runner");
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            None,
            "{status:?}"
        );
        assert_eq!(status.code(), Some(128 + signal));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !processes_with_marker(&marker).is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(processes_with_marker(&marker), Vec::<String>::new());
    }
}

/// A signal during a long `xsht lint --fix` ends the run between files or
/// fix rounds. Fixed files are written only after every file is done, so an
/// interrupted run leaves each file exactly as it was.
///
/// The signal goes to a spawned `xsht`. A cancellation request is state of
/// the whole process, and the tests of this target run as threads of one
/// process that call the same commands, so a request raised here would
/// interrupt whichever of them was running.
#[test]
fn lint_fix_cancellation_writes_no_file() {
    let mut functions = String::new();
    for index in 0..1000 {
        functions.push_str(&format!(
            "pure pick_{index}(n: Int) -> Int {{\n  let chosen = match n {{\n    1 => 10,\n    _ => 20,\n  }}\n  chosen + 1\n}}\n\n"
        ));
    }
    functions.push_str("print ${pick_0(1)}\n");
    for signal in [libc::SIGTERM, libc::SIGINT] {
        let root = TempDir::new().expect("temporary lint fixture");
        for file in 0..96 {
            fs::write(root.path().join(format!("module_{file}.xsh")), &functions)
                .expect("write lint fixture");
        }
        let mut lint = Command::new(release_bin!("xsht"))
            .args(["lint", "--fix", "."])
            .current_dir(root.path())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("start xsht lint");
        // Long enough for the handlers to be installed, far shorter than the
        // run: every file has a thousand fixes.
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            lint.try_wait().expect("poll lint").is_none(),
            "the lint run ended before it could be interrupted; the fixture is too small"
        );
        assert_eq!(unsafe { libc::kill(lint.id() as libc::pid_t, signal) }, 0);
        let started = std::time::Instant::now();
        let output = lint.wait_with_output().expect("wait for lint");
        assert_eq!(
            output.status.code(),
            Some(128 + signal),
            "{:?}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
        let name = if signal == libc::SIGINT { "SIGINT" } else { "SIGTERM" };
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&format!("interrupted by {name}")),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // One file's check and one fix round at most separate two looks at
        // the signal.
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
        for file in 0..96 {
            let path = root.path().join(format!("module_{file}.xsh"));
            assert!(
                fs::read_to_string(&path).expect("read lint fixture") == functions,
                "{} changed",
                path.display()
            );
        }
    }
}

#[test]
fn test_runner_times_out_hung_tests_and_stops_their_descendants() {
    let root = TempDir::new().expect("temporary timeout fixture");
    let marker = format!("xsht-timeout-{}", std::process::id());
    fs::create_dir(root.path().join("tests")).expect("create test root");
    fs::write(
        root.path().join("tests/hang.xsh"),
        format!(
            "\
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
"
        ),
    )
    .expect("write timeout fixture");
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
    std::io::Read::read_to_string(
        &mut runner.stdout.take().expect("runner stdout"),
        &mut stdout,
    )
    .expect("read runner stdout");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "{stdout}"
    );
    assert_eq!(status.code(), Some(1), "{stdout}");
    for name in [
        "spins",
        "sleeping_script",
        "sleeping_command",
        "own_shorter_limit",
    ] {
        assert!(
            stdout.contains(&format!("tests/hang.xsh::{name} ... TIMEOUT")),
            "{name}: {stdout}"
        );
    }
    assert!(
        stdout.contains(
            "---- tests/hang.xsh::spins ----\nstdout:\nspins cleanup ran\nTIMEOUT after 1s\n"
        ),
        "{stdout}"
    );
    assert!(stdout.contains("TIMEOUT after 200ms"), "{stdout}");
    assert!(
        stdout.contains("tests/hang.xsh::own_longer_limit ... ok"),
        "{stdout}"
    );
    assert!(stdout.contains("tests/hang.xsh::passes ... ok"), "{stdout}");
    assert!(
        stdout.contains("test result: FAILED. 2 passed; 4 failed; 0 skipped"),
        "{stdout}"
    );
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
    fs::write(
        root.path().join("tests/explicit.xsh"),
        "\
pure helper() -> Int { 2 }
proc ordinary_helper() { print helper() }
proc test_named_helper(value: Int) -> Int { value }
test test_old_name { assert test_named_helper(helper()) == 2 }
test no_prefix { |ctx| assert \"no_prefix\" in ctx.name }
test discarded { |_| }
",
    )
    .expect("write native declaration fixture");
    let list = Command::new(release_bin!("xsht"))
        .args(["test", "--list"])
        .current_dir(root.path())
        .output()
        .expect("list declarations");
    assert!(list.status.success());
    assert_eq!(
        String::from_utf8(list.stdout).unwrap(),
        "tests/explicit.xsh::discarded\ntests/explicit.xsh::no_prefix\ntests/explicit.xsh::test_old_name\n"
    );
    let run = Command::new(release_bin!("xsht"))
        .args(["test", "--jobs", "1"])
        .current_dir(root.path())
        .output()
        .expect("run declarations");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    assert!(String::from_utf8_lossy(&run.stdout).contains("3 passed; 0 failed"));
}

#[test]
fn native_test_declaration_legacy_proc_has_actionable_failure() {
    let root = TempDir::new().expect("temporary legacy fixture");
    fs::create_dir(root.path().join("tests")).expect("create test root");
    fs::write(
        root.path().join("tests/legacy.xsh"),
        "proc test_old(ctx: TestContext) -> Result[Unit] {}\n",
    )
    .expect("write legacy fixture");
    let run = Command::new(release_bin!("xsht"))
        .args(["test", "--jobs", "1"])
        .current_dir(root.path())
        .output()
        .expect("run legacy fixture");
    assert_eq!(run.status.code(), Some(1));
    let output = String::from_utf8_lossy(&run.stdout);
    assert!(output.contains("check.legacy-test-proc"), "{output}");
    assert!(output.contains("keep the exact declared name"), "{output}");
    assert!(!output.contains("0 passed; 0 failed"), "{output}");
    let filtered = Command::new(release_bin!("xsht"))
        .args(["test", "--exact", "tests/legacy.xsh::test_old"])
        .current_dir(root.path())
        .output()
        .expect("run exact legacy filter");
    assert_eq!(filtered.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&filtered.stdout).contains("check.legacy-test-proc"));
}

#[test]
fn native_test_declaration_import_registers_without_execution_or_discovery() {
    let root = TempDir::new().expect("temporary imported declaration fixture");
    fs::create_dir(root.path().join("tests")).expect("create test root");
    fs::write(root.path().join("tests/helper.xsh"), "##! Import registration fixture.\n## Returns the fixture value.\nexport pure value() -> Int { 7 }\ntest imported { assert false }\n")
        .expect("write imported module");
    fs::write(
        root.path().join("tests/entry.xsh"),
        "use helper\ntest entry { assert helper.value() == 7 }\n",
    )
    .expect("write entry fixture");
    let run = Command::new(release_bin!("xsht"))
        .args(["test", "--jobs", "1", "tests/entry.xsh"])
        .current_dir(root.path())
        .output()
        .expect("run imported fixture");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    let output = String::from_utf8_lossy(&run.stdout);
    assert!(output.contains("1 passed; 0 failed"), "{output}");
    assert!(!output.contains("::imported"), "{output}");
}

#[test]
fn enum_migration_fix_preserves_comments_aliases_and_imports() {
    let root = TempDir::new().expect("enum migration fixture");
    let module = root.path().join("choice.xsh");
    fs::write(&module, "##! Nominal choices.\n## A choice.\nexport type Choice =\n  Selected(Int) # selected café\n  | Empty # absent\n## Same nominal identity.\nexport type Alias = Choice\n").expect("write legacy enum module");
    let entry = root.path().join("entry.xsh");
    fs::write(&entry, "use choice as c\nlet value: c.Alias = c.Selected(7)\nmatch value { c.Selected(number) => print $number; c.Empty => print \"empty\" }\n").expect("write enum entry");
    let run = |arguments: &[&str]| {
        Command::new(release_bin!("xsht"))
            .args(arguments)
            .current_dir(root.path())
            .output()
            .expect("run enum fixture")
    };
    let rejected = run(&["check", "entry.xsh"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("parse.enum-migration"));
    let first = run(&["lint", "--fix", "entry.xsh"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let fixed = fs::read_to_string(&module).expect("read migrated module");
    for fragment in [
        "export enum Choice {",
        "# selected café",
        "# absent",
        "export type Alias = Choice",
    ] {
        assert!(fixed.contains(fragment), "{fixed}");
    }
    let second = run(&["lint", "--fix", "entry.xsh"]);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(
        fixed,
        fs::read_to_string(&module).expect("read stable module")
    );
    let checked = run(&["check", "entry.xsh"]);
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

#[test]
fn enum_migration_fix_retains_unrelated_checker_errors() {
    let root = TempDir::new().expect("enum rejected migration fixture");
    let entry = root.path().join("entry.xsh");
    let source = "type Choice = Selected(Int) | Empty\nlet value = Selected(\"wrong\")\n";
    fs::write(&entry, source).expect("write invalid enum use");
    let output = Command::new(release_bin!("xsht"))
        .args(["lint", "--fix", "entry.xsh"])
        .current_dir(root.path())
        .output()
        .expect("check migration candidate");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("check.type-mismatch"), "{stderr}");
    assert_eq!(
        source,
        fs::read_to_string(&entry).expect("read unchanged invalid source")
    );
}

#[test]
fn signature_cli_safe_fix_preserves_process_results_and_is_idempotent() {
    let root = TempDir::new().unwrap();
    let fixture = root.path().join("entry.xsh");
    fs::write(&fixture, "type Options = {jobs: Int, verbose: Bool}\nproc main(...argv: List[Str]) [error] {\n  let {jobs, verbose}: Options = cli.parse(argv, {jobs: {kind: \"Int\", default: 4, help: \"Int, default: 4\"}, verbose: {kind: \"Bool\", default: false, help: \"Bool, default: false\"}})?\n  let shown = Options(jobs:, verbose:)\n  print $shown.jobs $shown.verbose\n}\n").unwrap();
    let run = |arguments: &[&str]| {
        Command::new(release_bin!("xsht"))
            .args(arguments)
            .current_dir(root.path())
            .output()
            .unwrap()
    };
    let cases = [
        vec![],
        vec!["--jobs=8", "--verbose"],
        vec!["--help"],
        vec!["--jobs=invalid"],
        vec!["--jobs=2", "--jobs=3"],
    ];
    let invoke = |arguments: &Vec<&str>| {
        let mut command = vec!["trace", "entry.xsh", "--"];
        command.extend(arguments.iter().copied());
        let output = run(&command);
        (output.status.code(), output.stdout)
    };
    let before = cases.iter().map(invoke).collect::<Vec<_>>();
    let first = run(&["lint", "--fix", "entry.xsh"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let fixed = fs::read_to_string(&fixture).unwrap();
    assert!(fixed.contains("cli main("), "{fixed}");
    let second = run(&["lint", "--fix", "entry.xsh"]);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(fixed, fs::read_to_string(&fixture).unwrap());
    assert_eq!(before, cases.iter().map(invoke).collect::<Vec<_>>());
}

