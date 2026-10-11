use consolidation_metrics as metrics

pure value(scan: metrics.SourceScan, name: Str) -> Int {
  let rows = scan.metrics |> where .name == name |> collect
  rows[0].value
}

proc rejection(source: Str) [error] -> Str {
  match metrics.scan_file("src/fixture.rs", source) {
    Ok(_) => "accepted"
    Err(metrics.MetricError.Invalid {detail, ..}) => detail
    Err(error) => error.message
  }
}

pure schema_rows(start_ordinal: Int, end_ordinal: Int) -> Str {
  var rows = ""
  var ordinal = start_ordinal
  while ordinal < end_ordinal {
    rows = f"{rows}Variant{ordinal} => Tag{ordinal} = {ordinal} [0];\n"
    ordinal += 1
  }
  rows
}

proc measured(root: Path) [fs, error] -> Result[metrics.Report] {
  match metrics.measure(root, ["src", "crates/xsht/src"]) {
    Ok(report) => report
    Err(metrics.MetricError.Invalid {path: source_path, line, detail}) => fail f"{source_path}:{line}: {detail}"
    Err(error) => Err(error)
  }
}

proc tree(ctx: TestContext) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "structural-source")?
  fp"{root}/src".mkdir()
  fp"{root}/crates/xsht/src".mkdir()
  fp"{root}/crates/xsht/src/lib.rs".write("// tool root\n")
  root
}

test test_literal_delimiters_comments_and_lifetimes_do_not_own_items {
  let source = "fn live<'a>(s: &'a str) -> &'a str {\n let a = \"} // indexed_raw() \\\"\";\n let b = br###\"/* { indexed_finish() */\"###;\n let c = cr#\"} cfg(test)\"#;\n let d = b'}'; let e = '\\u{7d}';\n /* nested /* } */ indexed_decode() { */\n s\n}\n#[cfg(test)]\nmod tests {\n let a = r#\"} /*\"#;\n #[cfg(test)] mod nested { fn hidden() {} }\n}\n// retained comment\n\n"
  let scan = metrics.scan_file("src/fixture.rs", source)?
  assert scan.count.physical == 15
  assert scan.count.retained_lines == [1, 2, 3, 4, 5, 6, 7, 8, 14, 15]
  assert value(scan, "manual_payload_calls") == 0
  assert value(scan, "optional_raw_calls") == 0
}

test test_cfg_keeps_every_possible_non_test_assignment {
  let source = "#[cfg(any(target_os = \"linux\", test))]\nfn platform() {}\n#[cfg(all(test, unix))]\nfn hidden() {}\n#[cfg(all(unix, not(unix)))]\nfn impossible() {}\n#[cfg(not(test))]\nfn production() {}\n#[cfg_attr(test, allow(dead_code), derive(Debug))]\nstruct Metadata;\n"
  let scan = metrics.scan_file("src/fixture.rs", source)?
  assert scan.count.retained_lines == [1, 2, 7, 8, 9, 10]
  assert value(scan, "blanket_dead_code_lines") == 0
}

test test_cfg_predicates_share_assignments_across_attributes {
  let source = "#[cfg(unix)]\n#[cfg(not(unix))]\nfn impossible() {}\nfn live() {}\n"
  assert metrics.scan_file("src/fixture.rs", source)?.count.retained_lines == [4]
}

test test_physical_line_is_retained_when_any_production_token_intersects {
  let source = "#[cfg(test)] fn hidden() {} fn live() {}\n#[cfg(test)]\nfn other() {}\n\n// outside\n"
  let scan = metrics.scan_file("src/fixture.rs", source)?
  assert scan.count.physical == 5
  assert scan.count.retained_lines == [1, 4, 5]
}

test test_multiline_production_literal_retains_every_physical_line {
  let source = "const TEXT: &str = r#\"first\n#[cfg(test)] fn pretend() {}\nlast\"#;\n#[cfg(test)] fn hidden() {}\n"
  assert metrics.scan_file("src/fixture.rs", source)?.count.retained_lines == [1, 2, 3]
}

test test_external_test_modules_follow_path_and_normal_nested_module_rules { |ctx|
  let root = tree(ctx)?
  fp"{root}/src/feature".mkdir()
  fp"{root}/src/lib.rs".write("mod feature;\n#[cfg(test)]\n#[path = \"odd_tests.rs\"]\nmod tests;\n")
  fp"{root}/src/odd_tests.rs".write("fn hidden() {}\n")
  fp"{root}/src/feature.rs".write("#[cfg(test)]\nmod tests;\nfn live() {}\n")
  fp"{root}/src/feature/tests.rs".write("fn hidden_nested() {}\n")
  let report = measured(root)?
  let excluded = report.sources |> where .production == 0 |> map .path |> sort
  assert excluded == ["src/feature/tests.rs", "src/odd_tests.rs"]
  let source = report.metrics |> where .name == "source_lines_src" |> collect
  assert source[0].value == 2
  let encoded = json.encode(report)?
  assert json.decode(encoded)?.require(metrics.Report)? == report
}

test test_shared_external_module_remains_production_when_any_reference_is_production { |ctx|
  let root = tree(ctx)?
  fp"{root}/src/lib.rs".write("#[cfg(test)] #[path = \"shared.rs\"] mod tests;\n#[cfg(any(unix, test))] #[path = \"shared.rs\"] mod platform;\n")
  fp"{root}/src/shared.rs".write("fn live() {}\n")
  let report = measured(root)?
  let shared = report.sources |> where .path == "src/shared.rs" |> collect
  assert shared[0].production == 1
}

test test_inline_path_and_external_blanket_scopes_follow_module_ownership { |ctx|
  let root = tree(ctx)?
  fp"{root}/src/thread_files".mkdir()
  fp"{root}/src/lib.rs".write("#[allow(dead_code)]\n#[path = \"thread_files\"]\nmod thread { #[path = \"tls.rs\"] mod local_data; }\n")
  fp"{root}/src/thread_files/tls.rs".write("// inherited allowance\nfn unused() {}\n")
  let report = measured(root)?
  let blanket = report.metrics |> where .name == "blanket_dead_code_lines" |> collect
  assert blanket[0].value == 5
  assert (blanket[0].evidence |> where .path == "src/thread_files/tls.rs" |> count()) == 2
}

test test_inner_cfg_and_braced_constant_initializers_have_exact_boundaries {
  let source = "mod tests {\n#![cfg(test)]\nfn hidden() {}\n}\nfn live() {}\n#[cfg(test)] const VALUE: Foo = Foo { x: 1 }\n .field;\nfn other() {}\n"
  assert metrics.scan_file("src/fixture.rs", source)?.count.retained_lines == [5, 8]
  let generic = "#[cfg(test)] fn hidden<const N: usize>() { let value = 1; }\nfn live() {}\n"
  assert metrics.scan_file("src/fixture.rs", generic)?.count.retained_lines == [2]
}

test test_file_inner_cfg_removes_the_complete_file {
  let source = "// file comment\n#![cfg(test)]\n\nfn hidden() {}\n"
  assert metrics.scan_file("src/fixture.rs", source)?.count.production == 0
}

test test_only_tokens_contribute_calls_and_wide_alias_patterns {
  let source = "use ArenaPatternKind as P;\nfn live() { indexed_raw(x); indexed_decode::<bool>(x); indexed_finish(x); indexed_optional_raw(x);\nmatch kind { P::A => {}, P::B => {}, P::C => {}, P::D => {}, P::E => {}, P::F => {}, P::G => {}, P::H => {}, _ => {} }\n}\n#[cfg(test)] fn hidden() { indexed_raw(x); }\n"
  let scan = metrics.scan_file("src/fixture.rs", source)?
  assert value(scan, "manual_payload_calls") == 3
  assert value(scan, "optional_raw_calls") == 1
  assert value(scan, "wide_arena_wildcards") == 1
  let wide = scan.metrics |> where .name == "wide_arena_wildcards" |> collect
  assert wide[0].evidence[0].line == 3
  let tuple = "use ArenaPatternKind as P; fn live() { match (kind, flag) { (P::A, _) => {}, (P::B, _) => {}, (P::C, _) => {}, (P::D, _) => {}, (P::E, _) => {}, (P::F, _) => {}, (P::G, _) => {}, (P::H, _) => {}, _ => {} } }"
  assert value(metrics.scan_file("src/fixture.rs", tuple)?, "wide_arena_wildcards") == 1
}

test test_instruction_variants_support_hand_enums_and_generated_schema {
  let handwritten = "enum BuildIntRow { Int, Slot }\nenum BuildBoolRow { Bool }\nenum BuildExprRow { Null, Binary { left: u32, right: u32 } }\nenum BuildStmtRow { Break, Run { command: u32 } }\n"
  assert value(metrics.scan_file("src/fixture.rs", handwritten)?, "instruction_variants") == 7

  let generated = [
    "instruction_schema! {",
    "BuildIntRow => IntView [Int] {", "Int(value: i64) => IntInt = 0 [0; Operands];", "Slot(slot: usize) => IntSlot = 1 [0];", schema_rows(2, 6), "}",
    "BuildBoolRow => BoolView [Bool] {", "Bool(value: bool) => BoolBool = 6 [0];", "ContainsSlot { slot: usize, needle: Vec<(BuildExprId, Option<BuildExprId>)> } => BoolContainsSlot = 7 [EFFECT_PROCESS | EFFECT_TRACE; Scheduled];", schema_rows(8, 18), "}",
    "BuildExprRow => ExprView [Expr] {", "Null => ExprNull = 18 [0];", "Binary { op: BinaryOp, operands: Vec<(BuildExprId, Option<BuildExprId>)>, label: &'static str, span: Span } => ExprBinary = 19 [EFFECT_TRACE];", schema_rows(20, 115), "}",
    "BuildStmtRow => StmtView [Stmt] {", "Break => StmtBreak = 115 [0];", "Run { body: Vec<BuildStmtId>, span: Span } => StmtRun = 116 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_TRACE];", schema_rows(117, 158), "}",
    "}",
  ].join("\n")
  match metrics.scan_file("src/runtime/eval/indexed/full/schema.rs", generated) {
    Ok(schema) => {
      assert value(schema, "instruction_variants") == 158
      assert schema.count.production == generated.split("\n").len()
    }
    Err(metrics.MetricError.Invalid {detail, ..}) => test.fail(detail)
    Err(error) => test.fail(error.message)
  }
}

test test_instruction_schema_unknown_and_malformed_layouts_fail_loudly {
  assert "unknown instruction schema row" in rejection("instruction_schema! { BuildTaskRow => TaskView [Task] { Item => TaskItem = 0 [0]; } }")
  assert "malformed instruction schema entry" in rejection("instruction_schema! { BuildIntRow => IntView [Int] { Value Missing = 0 [0]; } }")
  assert "no supported table invocation" in rejection("macro_rules! instruction_schema { () => {}; }")
}

test test_malformed_lexical_forms_and_eligibility_cfg_attr_fail_loudly {
  assert "unterminated block comment" in rejection("/* open\n")
  assert "unterminated Rust literal" in rejection("fn f() { let x = r##\"open\"#; }")
  assert "mismatched Rust delimiters" in rejection("fn f() { ] }")
  assert "multi-character" in rejection("fn f() { let x = 'ab'; }")
  assert "unsupported Rust escape" in rejection("fn f() { let x = \"bad\\z\"; }")
  assert "invalid Unicode scalar" in rejection("fn f() { let x = '\\u{d800}'; }")
  assert "cfg_attr eligibility" in rejection("#[cfg_attr(test, cfg(unix))] fn f() {}")
  assert "cfg_attr eligibility" in rejection("#[cfg_attr(test, path = \"tests.rs\")] mod f;")
  assert "cfg not requires one" in rejection("#[cfg(not(test, unix))] fn f() {}")
}

test test_structural_rise_fails_with_the_metric_and_values { |ctx|
  let baseline: metrics.Report = {schema: 1, sources: [], metrics: [{name: "manual_payload_calls", value: 1, evidence: []}], reviews: []}
  let current: metrics.Report = {schema: 1, sources: [], metrics: [{name: "manual_payload_calls", value: 2, evidence: []}], reviews: []}
  match metrics.assert_not_risen(current, baseline) {
    Ok(_) => test.fail("rising structural metric passed")
    Err(metrics.MetricError.Rise {name, before, after}) => assert name == "manual_payload_calls" and before == 1 and after == 2
    Err(error) => test.fail(error.message)
  }
  metrics.assert_not_risen(baseline, current)?
  let root = tree(ctx)?; fp"{root}/dev/consolidation".mkdir()
  json.write(fp"{root}/base.json", baseline); json.write(fp"{root}/current.json", current)
  json.write(fp"{root}/dev/consolidation/baseline-metadata.json", {start_commit: "fixture"})
  let driver = p"dev/consolidation/metrics.xsh".read_text()?
  let flags = ["--root", root.display(), "--source-root", "src", "--xsht-source-root", "crates/xsht/src", "--input", fp"{root}/current.json".display(), "--baseline", fp"{root}/base.json".display(), "--output", fp"{root}/saved.json".display(), "--audit", fp"{root}/audit.json".display()]
  assert ! test.run_script(ctx, driver, ["check", @flags])?.success
  let approved = {metric: "manual_payload_calls", ceiling: 2, approval: "fixture owner", reason: "bounded fixture", remove_when: "fixture ends"}
  json.write(fp"{root}/envelope.json", {start_commit: "fixture", limits: [approved]})
  assert test.run_script(ctx, driver, ["check", @flags, "--envelope", fp"{root}/envelope.json".display()])?.success
  assert json.get(json.read(fp"{root}/audit.json")?, ["rises"])?.require(List[Any])?.len() == 1
  assert ! test.run_script(ctx, driver, ["final-close", @flags, "--envelope", fp"{root}/envelope.json".display()])?.success
  json.write(fp"{root}/envelope.json", {start_commit: "fixture", limits: [{...approved, metric: "unknown"}]}); assert ! test.run_script(ctx, driver, ["check", @flags, "--envelope", fp"{root}/envelope.json".display()])?.success
}

type ReviewFixture = {root: Path, commit: Str, sources: List[metrics.ReviewSource], artifact: metrics.ReviewArtifact}

const review_directory_paths = ["src", "crates", "tests", "core", "examples", "showcase", "tools", "dev/tests"]
const review_source_paths = [@review_directory_paths, "dev/consolidation_metrics.xsh", "dev/consolidation/metrics.xsh"]

proc git_output(root: Path, arguments: List[Str]) [process, error] -> Str {
  let argv = ["git", "-C", root.display(), @arguments]
  let output = run.capture --text --timeout=30s @argv
  assert output.status.shell_code()? == 0
  output.stdout.trim()
}

proc review_fixture(ctx: TestContext) [fs, process, error] -> Result[ReviewFixture] {
  let root = test.temp_dir(ctx, name: "review-evidence")?
  for source_path in review_directory_paths {
    fp"{root}/{source_path}".mkdir()
    fp"{root}/{source_path}/fixture.txt".write(f"fixture for {source_path}\n")
  }
  fp"{root}/src/lib.rs".write("fn fixture() {}\n")
  fp"{root}/crates/xsht/src".mkdir()
  fp"{root}/crates/xsht/src/lib.rs".write("// xsht fixture\n")
  fp"{root}/dev/consolidation".mkdir()
  fp"{root}/dev/consolidation_metrics.xsh".write("# metrics fixture\n")
  fp"{root}/dev/consolidation/metrics.xsh".write("# driver fixture\n")
  fp"{root}/.work".mkdir()
  fp"{root}/.work/proofs".mkdir()
  let _ = git_output(root, ["init", "--quiet"])
  let _ = git_output(root, ["add", "."])
  let _ = git_output(root, ["-c", "user.name=Fixture Reviewer", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "source fixture"])
  let commit = git_output(root, ["rev-parse", "HEAD"])
  let sources = collect {
    for source_path in review_source_paths {
      yield {path: source_path, oid: git_output(root, ["rev-parse", f"{commit}:{source_path}"])}
    }
  }
  let artifact_path = ".work/proofs/review.txt"
  fp"{root}/{artifact_path}".write("synthetic review evidence\n")
  let artifact: metrics.ReviewArtifact = {
    path: artifact_path,
    sha256: hash.sha256(fp"{root}/{artifact_path}")?.hex(),
    command: "xsht test dev/tests/test-consolidation-ratchets.xsh",
    method: "native fixture assertion",
    result: "verified",
  }
  {root: root, commit: commit, sources: sources, artifact: artifact}
}

pure close_report(src_lines: Int, xsht_lines: Int, blanket_lines: Int) -> metrics.Report {
  {
    schema: 1,
    sources: [],
    metrics: [
      {name: "source_lines_src", value: src_lines, evidence: []},
      {name: "source_lines_xsht", value: xsht_lines, evidence: []},
      {name: "blanket_dead_code_lines", value: blanket_lines, evidence: []},
    ],
    reviews: [],
  }
}

proc final_close_failure(current: metrics.Report, baseline: metrics.Report) [error] -> Str {
  match metrics.assert_final_close(current, baseline) {
    Ok(_) => "accepted"
    Err(metrics.MetricError.Contract {detail}) => detail
    Err(metrics.MetricError.Rise {name, ..}) => name
    Err(error) => error.message
  }
}

pure report_metric(report: metrics.Report, name: Str) -> Int {
  (report.metrics |> where .name == name |> collect)[0].value
}

pure proof_bundle(report: metrics.Report, fixture: ReviewFixture) -> metrics.ReviewEvidenceBundle {
  {
    schema: 1,
    source_commit: fixture.commit,
    sources: fixture.sources,
    rows: collect {
      for review in report.reviews {
        yield {
          name: review.name,
          status: "human_verified",
          reviewer: "fixture reviewer",
          evidence: [fixture.artifact],
        }
      }
    },
  }
}

proc review_failure(report: metrics.Report, bundle: metrics.ReviewEvidenceBundle, root: Path) [fs, process, error] -> Str {
  match metrics.verify_review_evidence(report, bundle, root) {
    Ok(_) => "accepted"
    Err(metrics.MetricError.Contract {detail}) => detail
    Err(error) => error.message
  }
}

test test_final_close_requires_strict_source_reduction_and_zero_blankets {
  let baseline = close_report(10, 8, 4)
  let equal_src = close_report(10, 7, 0)
  let equal_xsht = close_report(9, 8, 0)
  let blanket_remains = close_report(9, 7, 1)
  let closed = close_report(9, 7, 0)
  assert "source_lines_src" in final_close_failure(equal_src, baseline)
  assert "source_lines_xsht" in final_close_failure(equal_xsht, baseline)
  assert "blanket_dead_code_lines" in final_close_failure(blanket_remains, baseline)
  metrics.assert_final_close(closed, baseline)?
}

test test_review_evidence_requires_all_unique_verified_rows_and_hashed_workspace_artifacts { |ctx|
  let fixture = review_fixture(ctx)?
  let report = metrics.measure(fixture.root, ["src", "crates/xsht/src"])?
  let valid = proof_bundle(report, fixture)
  assert review_failure(report, valid, fixture.root) == "accepted"
  let missing = {...valid, rows: valid.rows |> where .name != "timings" |> collect}
  assert review_failure(report, missing, fixture.root) != "accepted"
  let duplicate = {...valid, rows: valid.rows + [valid.rows[0]]}
  assert review_failure(report, duplicate, fixture.root) != "accepted"
  let unresolved = {...valid, rows: collect { for row in valid.rows { yield if row.name == "timings" { {...row, status: "open"} } else { row } } }}
  assert review_failure(report, unresolved, fixture.root) != "accepted"
  let failed_result = {...valid, rows: collect { for row in valid.rows { yield if row.name == "timings" { {...row, evidence: [{...row.evidence[0], result: "failed"}]} } else { row } } }}
  assert review_failure(report, failed_result, fixture.root) != "accepted"
  let bad_hash = {...valid, rows: collect { for row in valid.rows { yield if row.name == "timings" { {...row, evidence: [{...row.evidence[0], sha256: "0000000000000000000000000000000000000000000000000000000000000000"}]} } else { row } } }}
  assert review_failure(report, bad_hash, fixture.root) != "accepted"
  let unreadable = {...valid, rows: collect { for row in valid.rows { yield if row.name == "timings" { {...row, evidence: [{...row.evidence[0], path: ".work/proofs/missing.txt"}]} } else { row } } }}
  assert review_failure(report, unreadable, fixture.root) != "accepted"
  let outside = {...valid, rows: collect { for row in valid.rows { yield if row.name == "timings" { {...row, evidence: [{...row.evidence[0], path: "../outside.txt"}]} } else { row } } }}
  assert review_failure(report, outside, fixture.root) != "accepted"
  let nonhex = {...valid, source_commit: "not-a-commit"}
  assert review_failure(report, nonhex, fixture.root) != "accepted"
  let stale_sources = {...valid, sources: collect { for source in valid.sources { yield if source.path == "src" { {...source, oid: "0000000000000000000000000000000000000000"} } else { source } } }}
  assert review_failure(report, stale_sources, fixture.root) != "accepted"
}

test test_review_evidence_accepts_synthetic_commit_after_docs_only_commit { |ctx|
  let fixture = review_fixture(ctx)?
  let report = metrics.measure(fixture.root, ["src", "crates/xsht/src"])?
  let proof = proof_bundle(report, fixture)
  fp"{fixture.root}/README.md".write("docs-only update\n")
  let _ = git_output(fixture.root, ["add", "README.md"])
  let _ = git_output(fixture.root, ["-c", "user.name=Fixture Reviewer", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "documentation only"])
  assert review_failure(report, proof, fixture.root) == "accepted"
  fp"{fixture.root}/src/lib.rs".write("fn changed_fixture() {}\n")
  let _ = git_output(fixture.root, ["add", "src/lib.rs"])
  let _ = git_output(fixture.root, ["-c", "user.name=Fixture Reviewer", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "changed source"])
  assert review_failure(report, proof, fixture.root) != "accepted"
}

test test_final_close_cli_requires_verified_bundle_and_independent_reductions { |ctx|
  let fixture = review_fixture(ctx)?
  let report = metrics.measure(fixture.root, ["src", "crates/xsht/src"])?
  let proof = proof_bundle(report, fixture)
  let baseline: metrics.Report = {
    ...report,
    metrics: collect {
      for row in report.metrics {
        yield if row.name in ["source_lines_src", "source_lines_xsht", "blanket_dead_code_lines"] { {...row, value: row.value + 1} } else { row }
      }
    },
  }
  let driver = p"dev/consolidation/metrics.xsh".read_text()?
  let proof_path = fp"{fixture.root}/.work/proofs/bundle.json"
  json.write(proof_path, proof)
  json.write(fp"{fixture.root}/current.json", report)
  json.write(fp"{fixture.root}/baseline.json", baseline)
  fp"{fixture.root}/dev/consolidation/baseline-metadata.json".write("{\"start_commit\":\"fixture\"}")
  let flags = ["--root", fixture.root.display(), "--source-root", "src", "--xsht-source-root", "crates/xsht/src", "--input", fp"{fixture.root}/current.json".display(), "--baseline", fp"{fixture.root}/baseline.json".display(), "--output", fp"{fixture.root}/saved.json".display(), "--audit", fp"{fixture.root}/audit.json".display()]
  assert ! test.run_script(ctx, driver, ["final-close", @flags])?.success
  assert test.run_script(ctx, driver, ["final-close", @flags, "--review-evidence", proof_path.display()])?.success
  let unresolved = {...proof, rows: collect { for row in proof.rows { yield if row.name == "timings" { {...row, status: "open"} } else { row } } }}
  json.write(proof_path, unresolved)
  assert ! test.run_script(ctx, driver, ["final-close", @flags, "--review-evidence", proof_path.display()])?.success
  let equal_src: metrics.Report = {
    ...report,
    metrics: collect {
      for row in report.metrics {
        yield if row.name == "source_lines_src" { {...row, value: report_metric(baseline, row.name)} } else { row }
      }
    },
  }
  json.write(fp"{fixture.root}/current.json", equal_src)
  json.write(proof_path, proof)
  assert ! test.run_script(ctx, driver, ["final-close", @flags, "--review-evidence", proof_path.display()])?.success
}
