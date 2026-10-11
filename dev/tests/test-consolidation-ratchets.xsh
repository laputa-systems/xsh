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
