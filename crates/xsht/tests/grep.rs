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
fn grep_reaches_boolean_guard_condition_and_failure_body() {
    let file = temp_xsh(
        "boolean_guard",
        "proc work(value: Str) [] { guard value.contains(\"ready\") else { abort(7) } }\n",
    );
    for pattern in ["RECEIVER.contains(EXPR)", "abort(EXPR)"] {
        let output = grep_scripts(pattern, &paths(&file));
        assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
        assert!(output_text(&output.stdout).contains("1 match"));
    }
    let _ = fs::remove_file(file);
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

    let output = Command::new(release_bin!("xsht"))
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

#[test]
fn grep_comparison_chain_matches_adjacent_operator_structure() {
    let source = include_str!("../../../tests/fixtures/frontend-indexed/comparison-chain.xsh");
    let file = temp_xsh("comparison_chain_structure", source);
    let matched = grep_scripts("A < B <= C", &paths(&file));
    assert_eq!(matched.status, 0, "{}", output_text(&matched.stderr));
    assert!(output_text(&matched.stdout).contains("1 < 2 <= 3"));
    let different = grep_scripts("A > B >= C", &paths(&file));
    assert_eq!(different.status, 1, "{}", output_text(&different.stderr));
}

#[test]
fn guarded_postfix_structural_matching_retains_each_guard() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("guarded.xsh");
    fs::write(&path, "let a = value?[0]\nlet b = value[0]\nlet c = value?[1..]\nlet d = value[1..]\nlet e = value?.trim()\nlet f = value.trim()\n").unwrap();
    for (pattern, expected, excluded) in [
        ("EXPR?[0]", "value?[0]", "value[0]"),
        ("EXPR?[1..]", "value?[1..]", "value[1..]"),
        ("EXPR?.trim()", "value?.trim()", "value.trim()"),
    ] {
        let output = Command::new(release_bin!("xsht"))
            .args(["grep", pattern])
            .arg(&path)
            .output()
            .unwrap();
        let stdout = output_text(&output.stdout);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            output_text(&output.stderr)
        );
        assert!(stdout.contains(expected), "{stdout}");
        assert!(!stdout.contains(excluded), "{stdout}");
    }
}

#[test]
fn grep_list_splicing_distinguishes_spliced_and_nested_elements() {
    let root = TempDir::new().expect("create temp root");
    let file = root.path().join("list-splicing.xsh");
    fs::write(
        &file,
        "let source = [1]\nlet nested = [source]\nlet spliced = [@source]\n",
    )
    .expect("write list fixture");
    for (pattern, expected, excluded) in [
        ("[@EXPR]", "[@source]", "[source]"),
        ("[EXPR]", "[source]", "[@source]"),
    ] {
        let output = grep_scripts(pattern, &paths(&file));
        let stdout = output_text(&output.stdout);
        assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
        assert!(stdout.contains(expected), "{stdout}");
        assert!(!stdout.contains(excluded), "{stdout}");
    }
}

#[test]
fn grep_list_pattern_tests_distinguish_exact_lengths_rest_and_nested_elements() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("list-pattern.xsh");
    fs::write(
        &file,
        include_str!("../../../tests/fixtures/syntax/list-pattern.xsh"),
    )
    .unwrap();
    let exact = grep_scripts("SUBJECT is [\"build\", _]", &paths(&file));
    assert_eq!(exact.status, 0, "{}", output_text(&exact.stderr));
    let stdout = output_text(&exact.stdout);
    assert!(stdout.contains("values is [\"build\", _]"));
    assert!(!stdout.contains("_, ..]"));
    assert!(!stdout.contains("clean"));
    let prefix = grep_scripts("SUBJECT is [\"build\", _, ..]", &paths(&file));
    assert_eq!(prefix.status, 0, "{}", output_text(&prefix.stderr));
    assert!(output_text(&prefix.stdout).contains("values is [\"build\", _, ..]"));
    let nested = grep_scripts("SUBJECT is [[_], [_]]", &paths(&file));
    assert_eq!(nested.status, 0, "{}", output_text(&nested.stderr));
    assert!(output_text(&nested.stdout).contains("[[1], [2]] is [[_], [_]]"));
}

#[test]
fn grep_regex_literal_compares_raw_patterns_across_delimiter_spellings() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("regex.xsh");
    fs::write(&file, "let single = rx\"[a-z]+\"\nlet triple = rx\"\"\"[a-z]+\"\"\"\nlet different = rx\"[0-9]+\"\n").unwrap();
    let output = grep_scripts("rx\"[a-z]+\"", &paths(&file));
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    let stdout = output_text(&output.stdout);
    assert!(stdout.contains("[a-z]+"));
    assert!(stdout.contains("2 matches"), "{stdout}");
    assert!(!stdout.contains("[0-9]+"));
}

#[test]
fn grep_visits_delegated_source_expressions_with_original_spans() {
    let file = temp_xsh(
        "yield_delegation_source",
        "stream rows() -> Stream[Int] { yield @(load(\"α\")?) }\n",
    );
    let output = grep_scripts("load(ARG)", &paths(&file));
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    assert!(output_text(&output.stdout).contains("load(\"α\")"));
}

#[test]
fn field_label_grep_and_refactor_preserve_keyword_key_identity() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("labels.xsh");
    fs::write(&file, "let bare = {type: \"file\"}\nlet quoted = {\"type\": \"file\"}\nlet different = {\"wire.type\": \"file\"}\nlet first = bare.type\nlet second = quoted.type\nprint $first $second\n").unwrap();
    let output = grep_scripts("EXPR.type", &paths(&file));
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    let stdout = output_text(&output.stdout);
    assert!(stdout.contains("2 matches"), "{stdout}");
    assert!(!stdout.contains("wire.type"), "{stdout}");
    let output = refactor_scripts("EXPR.type", "EXPR.type", &paths(&file), false);
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    let source = fs::read_to_string(&file).unwrap();
    let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
        xsh::frontend::source::SourceId::new(0),
        &source,
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(
        xsh::frontend::check::Checker::check_arena(&parsed.arena, &source)
            .diagnostics
            .is_empty()
    );
}

#[test]
fn grep_and_refactor_computed_map_entries_keep_static_labels_distinct() {
    let root = TempDir::new().expect("temporary workspace");
    let path = root.path().join("computed-map.xsh");
    fs::write(
        &path,
        "let key = \"one\"\nlet dynamic = {[key]: 1}\nlet fixed = {key: 1}\n",
    )
    .unwrap();
    let output = grep_scripts("{[KEY]: VALUE}", &paths(&path));
    let stdout = output_text(&output.stdout);
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    assert!(stdout.contains("{[key]: 1}"));
    assert!(!stdout.contains("{key: 1}"));
    let output = refactor_scripts(
        "{[KEY]: VALUE}",
        "{[KEY]: VALUE, [\"two\"]: 2}",
        &paths(&path),
        false,
    );
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    let updated = fs::read_to_string(&path).unwrap();
    assert!(updated.contains("{[key]: 1, [\"two\"]: 2}"), "{updated}");
    assert!(updated.contains("let fixed = {key: 1}"));
}

#[test]
fn grep_and_refactor_list_element_assignment_selectors_and_rhs() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("list-assignment.xsh");
    fs::write(
        &file,
        "var rows = [{count: 1}]\nrows[choose(0)].count += delta(2)\n",
    )
    .unwrap();
    for pattern in ["choose(EXPR)", "delta(EXPR)"] {
        let output = grep_scripts(pattern, &paths(&file));
        assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
        assert!(output_text(&output.stdout).contains("1 match"));
    }
    let output = refactor_scripts("choose(X)", "selected(X)", &paths(&file), false);
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    let fixed = fs::read_to_string(&file).unwrap();
    assert!(fixed.contains("rows[selected(0)].count += delta(2)"));
    let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
        xsh::frontend::source::SourceId::new(0),
        &fixed,
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
}

#[test]
fn grep_and_refactor_match_static_record_update_paths() {
    let source = "let base = {build: {jobs: 1}}\nlet next = {...base, build.jobs: 2}\nlet literal = {\"build.jobs\": 2}\n";
    let root = TempDir::new().expect("temp directory");
    let file = root.path().join("update.xsh");
    fs::write(&file, source).unwrap();
    let output = grep_scripts("{...BASE, build.jobs: VALUE}", &paths(&file));
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    assert!(output_text(&output.stdout).contains("1 match"));
    let output = refactor_scripts(
        "{...BASE, build.jobs: VALUE}",
        "{...BASE, build.jobs: changed(VALUE)}",
        &paths(&file),
        false,
    );
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    let updated = fs::read_to_string(&file).unwrap();
    assert!(
        updated.contains("{...base, build.jobs: changed(2)}"),
        "{updated}"
    );
    assert!(updated.contains("{\"build.jobs\": 2}"));
}

#[test]
fn grep_try_capture_matches_boundary_and_nested_call() {
    let file = temp_xsh("try_capture", "let result = try { load(\"α\")? }\n");
    for pattern in ["try { EXPR }", "load(ARG)"] {
        let output = grep_scripts(pattern, &paths(&file));
        assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
        assert!(output_text(&output.stdout).contains("load(\"α\")"));
    }
}

#[test]
fn refactor_try_capture_preserves_boundary_and_second_pass_is_empty() {
    let file = temp_xsh(
        "refactor_try_capture",
        "let result = try { load(\"α\")? }\n",
    );
    let output = refactor_scripts("load(ARG)", "read(ARG)", &paths(&file), false);
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    let fixed = fs::read_to_string(&file).unwrap();
    assert!(fixed.contains("try { read(\"α\")? }"), "{fixed}");
    let second = refactor_scripts("load(ARG)", "read(ARG)", &paths(&file), false);
    assert_eq!(second.status, 1, "{}", output_text(&second.stderr));
    assert_eq!(fs::read_to_string(&file).unwrap(), fixed);
}

#[test]
fn pattern_alternatives_grep_preserves_order_and_nested_shapes() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("pattern-alternatives.xsh");
    fs::write(&file, "let selected = 1 is (1 | 2)\nlet reversed = 1 is (2 | 1)\nlet nested = [1] is ([1] | [2])\n").unwrap();
    let output = grep_scripts("SUBJECT is (1 | 2)", &paths(&file));
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    let stdout = output_text(&output.stdout);
    assert!(stdout.contains("1 is (1 | 2)"));
    assert!(!stdout.contains("2 | 1"));
    assert!(!stdout.contains("[1] | [2]"));
}

#[test]
fn pattern_aliases_grep_matches_whole_subject_aliases_without_losing_precedence() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("pattern-aliases.xsh");
    fs::write(&file, "let whole = match 1 { (1 | 2) as original => original _ => 0 }\nlet separate = match 1 { 1 as original | 2 as original => original _ => 0 }\n").unwrap();
    let output = grep_scripts(
        "match SUBJECT { (1 | 2) as original => BODY _ => 0 }",
        &paths(&file),
    );
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    let stdout = output_text(&output.stdout);
    assert!(stdout.contains("(1 | 2) as original"));
    assert!(!stdout.contains("1 as original | 2 as original"));
}

#[test]
fn pattern_alternatives_refactor_uses_original_subject_span_and_converges() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("pattern-alternatives.xsh");
    fs::write(
        &file,
        "let value = 1 # café\nlet selected = value is (1 | 2)\nlet other = value is (2 | 3)\n",
    )
    .unwrap();
    let output = refactor_scripts(
        "SUBJECT is (1 | 2)",
        "SUBJECT is (1 | 2 | 3)",
        &paths(&file),
        false,
    );
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    let fixed = fs::read_to_string(&file).unwrap();
    assert!(fixed.contains("value is (1 | 2 | 3)"));
    assert!(fixed.contains("value is (2 | 3)"));
    let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
        xsh::frontend::source::SourceId::new(0),
        &fixed,
    );
    assert!(parsed.diagnostics.is_empty());
    assert!(
        xsh::frontend::check::Checker::check_arena(&parsed.arena, &fixed)
            .diagnostics
            .is_empty()
    );
    let again = refactor_scripts(
        "SUBJECT is (1 | 2)",
        "SUBJECT is (1 | 2 | 3)",
        &paths(&file),
        false,
    );
    assert_eq!(again.status, 1);
    assert_eq!(fs::read_to_string(&file).unwrap(), fixed);
}

#[test]
fn grep_and_refactor_value_pipeline_holes_preserve_explicit_argument_placement() {
    let source = "let text = \"é\" |> render(\"[\", value: _)\n";
    let file = temp_xsh("pipeline_hole", source);
    let output = grep_scripts("INPUT |> render(PREFIX, value: _)", &paths(&file));
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    assert!(output_text(&output.stdout).contains("\"é\" |> render(\"[\", value: _)"));
    let wrong_name = grep_scripts("INPUT |> render(PREFIX, alternate: _)", &paths(&file));
    assert_eq!(wrong_name.status, 1, "{}", output_text(&wrong_name.stdout));
    let output = refactor_scripts(
        "INPUT |> render(PREFIX, value: _)",
        "INPUT |> render(PREFIX, value: _)",
        &paths(&file),
        false,
    );
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    assert_eq!(fs::read_to_string(file).unwrap(), source);
}

#[test]
fn refactor_value_pipeline_holes_retains_optional_calls_and_result_boundaries() {
    for (name, source, pattern) in [
        (
            "pipeline_result",
            "let value = \"3\" |> parse(_, radix: 10)?\n",
            "INPUT |> parse(_, radix: RADIX)?",
        ),
        (
            "pipeline_optional",
            "let value = \"a\" |> maybe?.replace(_, \"b\")\n",
            "INPUT |> RECEIVER?.replace(_, OTHER)",
        ),
    ] {
        let file = temp_xsh(name, source);
        let output = refactor_scripts(pattern, pattern, &paths(&file), false);
        assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
        let fixed = fs::read_to_string(&file).unwrap();
        assert!(
            !fixed.contains("INPUT")
                && !fixed.contains("RADIX")
                && !fixed.contains("RECEIVER")
                && !fixed.contains("OTHER"),
            "{fixed}"
        );
        let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
            xsh::frontend::source::SourceId::new(0),
            &fixed,
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(
            fixed.contains("|>") && fixed.contains('_') && fixed.contains('?'),
            "{fixed}"
        );
    }
}

#[test]
fn grep_and_refactor_visit_named_stream_configuration_and_spread_values() {
    let root = TempDir::new().expect("create stage configuration fixture");
    let file = root.path().join("stage-config.xsh");
    fs::write(&file, "let values = [1] |> par-map(jobs: worker_limit(1)) { |item| item } |> sort(...{desc: direction(true)}) # retain\n").unwrap();
    for query in ["worker_limit(EXPR)", "direction(EXPR)"] {
        let found = grep_scripts(query, &paths(&file));
        assert_eq!(found.status, 0, "{}", output_text(&found.stderr));
        assert!(output_text(&found.stdout).contains("1 match"));
    }
    let changed = refactor_scripts(
        "worker_limit(X)",
        "bounded_workers(X)",
        &paths(&file),
        false,
    );
    assert_eq!(changed.status, 0, "{}", output_text(&changed.stderr));
    let rewritten = fs::read_to_string(&file).unwrap();
    assert!(
        rewritten.contains("jobs: bounded_workers(1)"),
        "{rewritten}"
    );
    assert!(
        rewritten.contains("sort(...{desc: direction(true)})"),
        "{rewritten}"
    );
    assert!(rewritten.contains("# retain"));
}

#[test]
fn grep_and_refactor_visit_static_stage_callable_descriptors() {
    let root = TempDir::new().expect("create stage callable fixture");
    let file = root.path().join("stage-callable.xsh");
    fs::write(&file, "pure normalize(item: Str) -> Str { item.lower() }\npure canonicalize(item: Str) -> Str { item.upper() }\nlet values = [\"a\"] |> map(block: normalize) # retain descriptor comment\nprint values[0]\n").unwrap();
    let found = grep_scripts("normalize", &paths(&file));
    assert_eq!(found.status, 0, "{}", output_text(&found.stderr));
    assert!(output_text(&found.stdout).contains("1 match"));
    let changed = refactor_scripts("normalize", "canonicalize", &paths(&file), false);
    assert_eq!(changed.status, 0, "{}", output_text(&changed.stderr));
    let rewritten = fs::read_to_string(&file).unwrap();
    assert!(
        rewritten.contains("map(block: canonicalize)"),
        "{rewritten}"
    );
    assert!(rewritten.contains("# retain descriptor comment"));
}

#[test]
fn core_assert_structural_search_finds_condition_and_message_calls() {
    let directory = TempDir::new().unwrap();
    let script = directory.path().join("assert.xsh");
    fs::write(&script, "assert observed(1) == observed(2), observed(3)\n").unwrap();
    let output = grep_scripts("observed(EXPR)", &paths(&script));
    let stdout = output_text(&output.stdout);
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    assert!(stdout.contains("observed(1)"), "{stdout}");
    assert!(stdout.contains("observed(2)"), "{stdout}");
    assert!(stdout.contains("observed(3)"), "{stdout}");
    assert!(stdout.contains("3 matches"), "{stdout}");
}

#[test]
fn selective_retry_grep_and_refactor_preserve_filter_order_and_body() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("selective-retry.xsh");
    fs::write(&file, "let result = retry [0ms] on (FetchError.Busy | FetchError.Timeout) { fetch(\"café\")? }\nlet all = retry [0ms] { fetch(\"all\")? }\n").unwrap();
    let pattern = "retry [DELAY] on (FetchError.Busy | FetchError.Timeout) { fetch(ARG)? }";
    let output = grep_scripts(pattern, &paths(&file));
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    assert!(output_text(&output.stdout).contains("1 match"));
    let output = refactor_scripts(
        pattern,
        "retry [DELAY] on (FetchError.Busy | FetchError.Timeout) { load(ARG)? }",
        &paths(&file),
        false,
    );
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    let fixed = fs::read_to_string(&file).unwrap();
    assert!(fixed.contains("{ load(\"café\")? }"), "{fixed}");
    assert!(fixed.contains("retry [0ms] { fetch(\"all\")? }"));
    let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
        xsh::frontend::source::SourceId::new(0),
        &fixed,
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
}

#[test]
fn duration_arithmetic_grep_preserves_units_and_operand_order() {
    let file = temp_xsh(
        "duration_arithmetic",
        "let first = 250ms * 3\nlet second = 3 * 250ms\nlet other = 250s * 3\n",
    );
    let output = grep_scripts("250ms * COUNT", &paths(&file));
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    let stdout = output_text(&output.stdout);
    assert!(stdout.contains("250ms * 3"), "{stdout}");
    assert!(!stdout.contains("3 * 250ms"), "{stdout}");
    assert!(!stdout.contains("250s * 3"), "{stdout}");
}

#[test]
fn grep_and_refactor_visit_named_spread_operands() {
    let source = "pure sum(first: Int, second: Int) -> Int { first + second }\npure options() -> Pair { Pair(first: 2, second: 3) }\ntype Pair = {first: Int, second: Int}\nprint ${sum(...options())}\n";
    let file = temp_xsh("named_spread_operand", source);
    let found = grep_scripts("options()", &paths(&file));
    assert_eq!(found.status, 0, "{}", output_text(&found.stderr));
    assert!(output_text(&found.stdout).contains("options()"));
    let replaced = refactor_scripts(
        "options()",
        "Pair(first: 4, second: 5)",
        &paths(&file),
        false,
    );
    assert_eq!(replaced.status, 0, "{}", output_text(&replaced.stderr));
    let changed = fs::read_to_string(&file).unwrap();
    assert!(
        changed.contains("sum(...Pair(first: 4, second: 5))"),
        "{changed}"
    );
    fs::remove_file(file).unwrap();
}

#[test]
fn named_argument_spread_matching_retains_splice_and_label_identity() {
    let file = temp_xsh(
        "spread_arg_identity",
        "let options = {first: 1}\nf(...options)\nf(@options)\nf(options)\nf(first: options)\n",
    );
    let spread = grep_scripts("f(...EXPR)", &paths(&file));
    assert_eq!(spread.status, 0);
    let text = output_text(&spread.stdout);
    assert!(text.contains("f(...options)"));
    assert!(!text.contains("f(@options)"));
    assert!(!text.contains("f(first: options)"));
    let changed = refactor_scripts("f(...EXPR)", "g(...EXPR)", &paths(&file), false);
    assert_eq!(changed.status, 0);
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("g(...options)"));
    assert!(text.contains("f(@options)"));
    assert!(text.contains("f(first: options)"));
    fs::remove_file(file).unwrap();
}

#[test]
fn lexical_ctx_structural_grep_and_refactor_preserve_description_and_body() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("context.xsh");
    fs::write(
        &file,
        "let selected = ctx \"café\" { 7 }\nlet other = ctx \"other\" { 9 }\n",
    )
    .unwrap();
    let output = grep_scripts("ctx \"café\" { BODY }", &paths(&file));
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    assert!(output_text(&output.stdout).contains("ctx \"café\" { 7 }"));
    assert!(!output_text(&output.stdout).contains("other"));
    let fixed = refactor_scripts(
        "ctx \"café\" { BODY }",
        "ctx \"café updated\" { BODY }",
        &paths(&file),
        false,
    );
    assert_eq!(fixed.status, 0, "{}", output_text(&fixed.stderr));
    let source = fs::read_to_string(&file).unwrap();
    assert!(source.contains("ctx \"café updated\" { 7 }"));
    assert_eq!(
        refactor_scripts(
            "ctx \"café\" { BODY }",
            "ctx \"café updated\" { BODY }",
            &paths(&file),
            false
        )
        .status,
        1
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
}

#[test]
fn typed_map_keys_grep_refactor_preserve_computed_domains() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("typed-map.xsh");
    fs::write(
        &path,
        "var entries: Map[Int, Str] = {[20]: \"twenty\", [3]: \"three\"}\n",
    )
    .unwrap();
    let found = grep_scripts("{[KEY]: VALUE, [OTHER]: REST}", &paths(&path));
    assert_eq!(found.status, 0, "{}", output_text(&found.stderr));
    assert!(output_text(&found.stdout).contains("[20]"));
    let output = refactor_scripts(
        "{[KEY]: VALUE, [OTHER]: REST}",
        "{[OTHER]: REST, [KEY]: VALUE}",
        &paths(&path),
        false,
    );
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    let updated = fs::read_to_string(&path).unwrap();
    assert!(updated.contains("Map[Int, Str]"));
    assert!(
        updated.contains("{[3]: \"three\", [20]: \"twenty\"}"),
        "{updated}"
    );
}

#[test]
fn scalar_iteration_structural_tools_keep_loop_and_comprehension_sources() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("scalar-iteration.xsh");
    fs::write(&file, "# Unicode loop\nfor character in \"é\" { print $character }\nlet octets = [octet for octet in b\"\\x00\\xff\"]\n").unwrap();
    let output = grep_scripts("\"é\"", &paths(&file));
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    assert!(output_text(&output.stdout).contains("1 match"));
    let output = refactor_scripts("\"é\"", "\"🙂\"", &paths(&file), false);
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    let fixed = fs::read_to_string(&file).unwrap();
    assert!(fixed.contains("# Unicode loop\nfor character in \"🙂\""));
    assert!(fixed.contains("[octet for octet in b\"\\x00\\xff\"]"));
    let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
        xsh::frontend::source::SourceId::new(0),
        &fixed,
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(
        xsh::frontend::check::Checker::check_arena(&parsed.arena, &fixed)
            .diagnostics
            .is_empty()
    );
    let output = refactor_scripts("\"é\"", "\"🙂\"", &paths(&file), false);
    assert_eq!(output.status, 1);
    assert_eq!(fs::read_to_string(&file).unwrap(), fixed);
}

#[test]
fn typed_cause_grep_and_refactor_keep_named_operand() {
    let source = "error Outer = Failed(message: Str)\nerror Inner = Failed(message: Str)\nlet value = Err(Outer.Failed(message: \"outer\"), cause: Inner.Failed(message: \"inner\"))\n";
    let file = temp_xsh("typed_cause", source);
    let found = grep_scripts("Err(OUTER, cause: CAUSE)", &paths(&file));
    assert_eq!(found.status, 0, "{}", output_text(&found.stderr));
    assert!(output_text(&found.stdout).contains("cause: Inner.Failed"));
    let fixed = refactor_scripts(
        "Inner.Failed(message: VALUE)",
        "Inner.Failed(message: VALUE)",
        &paths(&file),
        false,
    );
    assert_eq!(fixed.status, 0, "{}", output_text(&fixed.stderr));
    let updated = fs::read_to_string(&file).unwrap();
    assert!(updated.contains("cause: Inner.Failed(message: \"inner\")"));
    let _ = fs::remove_file(file);
}

#[test]
fn grep_and_refactor_reach_accept_policy_expressions() {
    let root = TempDir::new().expect("temporary policy scripts");
    let file = root.path().join("accept.xsh");
    fs::write(&file, "pure choose_codes(codes: List[Int]) -> List[Int] { codes }\nrun.status --accept=choose_codes([0,1]) sh\n").unwrap();
    let found = grep_scripts("choose_codes(EXPR)", &paths(&file));
    assert_eq!(found.status, 0, "{}", output_text(&found.stderr));
    let replaced = refactor_scripts("choose_codes(EXPR)", "EXPR", &paths(&file), false);
    assert_eq!(replaced.status, 0, "{}", output_text(&replaced.stderr));
    let updated = fs::read_to_string(&file).unwrap();
    assert!(updated.contains("--accept=[0,1] sh"), "{updated}");
    let again = grep_scripts("choose_codes(EXPR)", &paths(&file));
    assert_eq!(again.status, 1);
}

#[test]
fn grep_and_refactor_reach_wire_enum_mapping_expressions() {
    let root = TempDir::new().expect("temporary source root");
    let file = root.path().join("state.xsh");
    let source = "enum State: Str { Ready = \"rea\" + \"dy\", Empty = \"\" }\nlet state: State = Ready\nprint json.encode(state)?\n";
    fs::write(&file, source).unwrap();
    let matched = grep_scripts("LEFT + RIGHT", &paths(&file));
    assert_eq!(matched.status, 0, "{}", output_text(&matched.stderr));
    assert!(output_text(&matched.stdout).contains("1 match"));
    let edited = refactor_scripts("\"rea\" + \"dy\"", "\"ready\"", &paths(&file), false);
    assert_eq!(edited.status, 0, "{}", output_text(&edited.stderr));
    let fixed = fs::read_to_string(&file).unwrap();
    assert!(fixed.contains("Ready = \"ready\""), "{fixed}");
    assert!(fixed.contains("Empty = \"\""), "{fixed}");
    let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
        xsh::frontend::source::SourceId::new(0),
        &fixed,
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(
        xsh::frontend::check::Checker::check_arena(&parsed.arena, &fixed)
            .diagnostics
            .is_empty()
    );
}

#[test]
fn context_scope_grep_and_refactor_preserve_input_body_and_scope_kind() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("scope.xsh");
    fs::write(
        &file,
        "let selected = env ({X: 7}) { 9 }\nlet other = cd (p\".\") { 9 }\n",
    )
    .unwrap();
    let output = grep_scripts("env (EXPR) { BODY }", &paths(&file));
    assert_eq!(output.status, 0, "{}", output_text(&output.stderr));
    assert!(output_text(&output.stdout).contains("env ({X: 7}) { 9 }"));
    assert!(!output_text(&output.stdout).contains("cd ("));
    let fixed = refactor_scripts(
        "env (EXPR) { BODY }",
        "env (EXPR) { BODY }",
        &paths(&file),
        false,
    );
    assert_eq!(fixed.status, 0, "{}", output_text(&fixed.stderr));
    let source = fs::read_to_string(&file).unwrap();
    assert!(source.contains("env ({X: 7}) {"));
    assert!(source.contains("cd (p\".\") {"));
}
