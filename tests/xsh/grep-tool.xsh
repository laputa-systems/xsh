# `xsht grep` matches an expression pattern against the syntax of each file and
# `xsht refactor` rewrites the matches from their original source spans. Both
# exit with 0 when something matched and 1 when nothing did.

proc script(ctx: TestContext, name: Str, source: Str) [fs, error] -> Result[Path] {
  test.temp_file(ctx, name:, contents: bytes.from_text(source))
}

proc found(pattern: Str, file: Path) [process, error] -> Result[Str] {
  let output = run.capture --text "xsht" grep $pattern $file
  assert output.status.exited_with(0), f"{pattern}: {output.stdout}{output.stderr}"
  output.stdout
}

proc absent(pattern: Str, file: Path) [process, error] -> Result[Str] {
  let output = run.capture --text "xsht" grep $pattern $file
  assert output.status.exited_with(1), f"{pattern}: {output.stdout}{output.stderr}"
  output.stdout
}

proc rewritten(pattern: Str, replacement: Str, file: Path) [fs, process, error] -> Result[Str] {
  let output = run.capture --text "xsht" refactor $pattern $replacement $file
  assert output.status.exited_with(0), f"{pattern}: {output.stdout}{output.stderr}"
  file.read_text()
}

proc unchanged(pattern: Str, replacement: Str, file: Path) [fs, process, error] -> Result[Str] {
  let output = run.capture --text "xsht" refactor $pattern $replacement $file
  assert output.status.exited_with(1), f"{pattern}: {output.stdout}{output.stderr}"
  file.read_text()
}

# A rewritten file has to stay a program: `xsht desugar` reports every parse
# diagnostic and nothing else, so it accepts sources that name helpers the
# fixture does not declare.
proc assert_parses(file: Path) [process, error] {
  let output = run.capture --text "xsht" desugar $file
  assert output.status.exited_with(0), output.stderr
  assert output.stderr == "", output.stderr
}

proc assert_checks(file: Path) [process, error] {
  let output = run.capture --text "xsht" check $file
  assert output.status.exited_with(0), f"{output.stdout}{output.stderr}"
}

test test_grep_reaches_boolean_guard_condition_and_failure_body { |ctx|
  let file = script(
    ctx,
    "boolean-guard.xsh",
    "proc work(value: Str) [] { guard value.contains(\"ready\") else { fail(7) } }\n",
  )?
  for pattern in ["RECEIVER.contains(EXPR)", "fail(EXPR)"] {
    let stdout = found(pattern, file)?
    assert "1 match" in stdout, stdout
  }
}

test test_grep_without_paths_uses_configured_includes { |ctx|
  let root = test.temp_dir(ctx, name: "grep-includes")?
  let scripts = fp"{root}/.github/scripts"
  scripts.mkdir()
  fp"{root}/xsht-config.ini".write("include = .github/scripts\n")
  fp"{scripts}/release.xsh".write("let items = [1]\nlet count = list.len(items)\n")
  let output = cd (root) { run.capture --text "xsht" grep "list.len(EXPR)" }?
  assert output.status.exited_with(0), output.stdout
  assert ".github/scripts/release.xsh" in output.stdout, output.stdout
  assert "list.len(items)" in output.stdout, output.stdout
}

test test_grep_reports_module_function_calls_and_exits_one_without_a_match { |ctx|
  let file = script(ctx, "module-call.xsh", "let xs = [1, 2, 3]\nlet n = list.len(xs)\n")?
  let stdout = found("list.len(EXPR)", file)?
  assert "list.len(xs)" in stdout, stdout
  assert "1 match" in stdout, stdout
  let missing = absent("map.get(M, K)", file)?
  assert "0 matches" in missing, missing
}

test test_grep_matches_method_calls { |ctx|
  let stdout = script(ctx, "method-call.xsh", "let xs = []\nxs.push(v)\n")? |> found("RECV.push(ITEM)", _)?
  assert "xs.push(v)" in stdout, stdout
}

test test_grep_reaches_calls_inside_pipeline_blocks { |ctx|
  let stdout = script(
    ctx,
    "pipeline-block.xsh",
    "let items = []\nlet result = items |> map { hash.sha256(item)? }\n",
  )? |> found("hash.sha256(P)", _)?
  assert "hash.sha256(item)" in stdout, stdout
}

# A pattern without `?` still matches the call a propagation wraps.
test test_grep_reaches_calls_inside_propagation { |ctx|
  let stdout = script(ctx, "inside-try.xsh", "let h = hash.sha256(p)?\n")? |> found("hash.sha256(P)", _)?
  assert "hash.sha256(p)" in stdout, stdout
}

test test_grep_without_a_match_exits_one { |ctx|
  let stdout = script(ctx, "no-matches.xsh", "let x = 1\n")? |> absent("list.len(EXPR)", _)?
  assert "0 matches" in stdout, stdout
}

test test_grep_binds_several_metavariables { |ctx|
  let stdout = script(ctx, "metavariables.xsh", "map.set(m, k, v)\n")? |> found("map.set(M, K, V)", _)?
  assert "map.set(m, k, v)" in stdout, stdout
  assert "1 match" in stdout, stdout
}

test test_refactor_rewrites_the_matched_call { |ctx|
  let fixed = script(ctx, "refactor-rename.xsh", "let n = list.len(xs)\n")? |> rewritten("list.len(X)", "X.len()", _)?
  assert "xs.len()" in fixed, fixed
  assert "list.len(xs)" not in fixed, fixed
}

test test_refactor_dry_run_leaves_the_file_unchanged { |ctx|
  let source = "let n = list.len(xs)\n"
  let file = script(ctx, "refactor-dry-run.xsh", source)?
  let output = run.capture --text "xsht" refactor --dry-run "list.len(X)" "X.len()" $file
  assert output.status.exited_with(0), output.stderr
  assert "dry run" in output.stdout, output.stdout
  assert file.read_text()? == source
}

test test_refactor_without_a_match_exits_one_and_leaves_the_file_unchanged { |ctx|
  let source = "let x = 1\n"
  let file = script(ctx, "refactor-noop.xsh", source)?
  assert unchanged("list.len(X)", "X.len()", file)? == source
}

test test_grep_comparison_chain_matches_adjacent_operator_structure { |ctx|
  let file = script(
    ctx,
    "comparison-chain.xsh",
    p"tests/fixtures/frontend-indexed/comparison-chain.xsh".read_text()?,
  )?
  let stdout = found("A < B <= C", file)?
  assert "1 < 2 <= 3" in stdout, stdout
  let _ = absent("A > B >= C", file)?
}

test test_grep_guarded_postfix_patterns_retain_each_guard { |ctx|
  let file = script(
    ctx,
    "guarded.xsh",
    "let a = value?[0]\nlet b = value[0]\nlet c = value?[1..]\nlet d = value[1..]\nlet e = value?.trim()\nlet f = value.trim()\n",
  )?
  for {pattern, expected, excluded} in [
    {pattern: "EXPR?[0]", expected: "value?[0]", excluded: "value[0]"},
    {pattern: "EXPR?[1..]", expected: "value?[1..]", excluded: "value[1..]"},
    {pattern: "EXPR?.trim()", expected: "value?.trim()", excluded: "value.trim()"},
  ] {
    let stdout = found(pattern, file)?
    assert expected in stdout, stdout
    assert excluded not in stdout, stdout
  }
}

test test_grep_list_splicing_distinguishes_spliced_and_nested_elements { |ctx|
  let file = script(ctx, "list-splicing.xsh", "let source = [1]\nlet nested = [source]\nlet spliced = [@source]\n")?
  for {pattern, expected, excluded} in [
    {pattern: "[@EXPR]", expected: "[@source]", excluded: "[source]"},
    {pattern: "[EXPR]", expected: "[source]", excluded: "[@source]"},
  ] {
    let stdout = found(pattern, file)?
    assert expected in stdout, stdout
    assert excluded not in stdout, stdout
  }
}

test test_grep_list_pattern_tests_distinguish_exact_lengths_rest_and_nested_elements { |ctx|
  let file = script(ctx, "list-pattern.xsh", p"tests/fixtures/syntax/list-pattern.xsh".read_text()?)?
  let exact = found("SUBJECT is [\"build\", _]", file)?
  assert "values is [\"build\", _]" in exact, exact
  assert "_, ..]" not in exact, exact
  assert "clean" not in exact, exact
  let prefix = found("SUBJECT is [\"build\", _, ..]", file)?
  assert "values is [\"build\", _, ..]" in prefix, prefix
  let nested = found("SUBJECT is [[_], [_]]", file)?
  assert "[[1], [2]] is [[_], [_]]" in nested, nested
}

test test_grep_regex_literal_compares_raw_patterns_across_delimiter_spellings { |ctx|
  let stdout = script(
    ctx,
    "regex.xsh",
    "let single = rx\"[a-z]+\"\nlet triple = rx\"\"\"[a-z]+\"\"\"\nlet different = rx\"[0-9]+\"\n",
  )? |> found("rx\"[a-z]+\"", _)?
  assert "[a-z]+" in stdout, stdout
  assert "2 matches" in stdout, stdout
  assert "[0-9]+" not in stdout, stdout
}

test test_grep_visits_delegated_source_expressions_with_original_spans { |ctx|
  let stdout = script(
    ctx,
    "yield-delegation.xsh",
    "stream rows() -> Stream[Int] { yield @(load(\"α\")?) }\n",
  )? |> found("load(ARG)", _)?
  assert "load(\"α\")" in stdout, stdout
}

test test_grep_and_refactor_field_labels_preserve_keyword_key_identity { |ctx|
  let file = script(
    ctx,
    "labels.xsh",
    r"""let bare = {type: "file"}
let quoted = {"type": "file"}
let different = {"wire.type": "file"}
let first = bare.type
let second = quoted.type
print $first $second
""",
  )?
  let stdout = found("EXPR.type", file)?
  assert "2 matches" in stdout, stdout
  assert "wire.type" not in stdout, stdout
  let _ = rewritten("EXPR.type", "EXPR.type", file)?
  assert_checks(file)
}

test test_grep_and_refactor_computed_map_entries_keep_static_labels_distinct { |ctx|
  let file = script(ctx, "computed-map.xsh", "let key = \"one\"\nlet dynamic = {[key]: 1}\nlet fixed = {key: 1}\n")?
  let stdout = found("{[KEY]: VALUE}", file)?
  assert "{[key]: 1}" in stdout, stdout
  assert "{key: 1}" not in stdout, stdout
  let updated = rewritten("{[KEY]: VALUE}", "{[KEY]: VALUE, [\"two\"]: 2}", file)?
  assert "{[key]: 1, [\"two\"]: 2}" in updated, updated
  assert "let fixed = {key: 1}" in updated, updated
}

test test_grep_and_refactor_list_element_assignment_selectors_and_rhs { |ctx|
  let file = script(ctx, "list-assignment.xsh", "var rows = [{count: 1}]\nrows[choose(0)].count += delta(2)\n")?
  for pattern in ["choose(EXPR)", "delta(EXPR)"] {
    let stdout = found(pattern, file)?
    assert "1 match" in stdout, stdout
  }

  let fixed = rewritten("choose(X)", "selected(X)", file)?
  assert "rows[selected(0)].count += delta(2)" in fixed, fixed
  assert_parses(file)
}

test test_grep_and_refactor_match_static_record_update_paths { |ctx|
  let file = script(
    ctx,
    "update.xsh",
    "let base = {build: {jobs: 1}}\nlet next = {...base, build.jobs: 2}\nlet literal = {\"build.jobs\": 2}\n",
  )?
  let stdout = found("{...BASE, build.jobs: VALUE}", file)?
  assert "1 match" in stdout, stdout
  let updated = rewritten("{...BASE, build.jobs: VALUE}", "{...BASE, build.jobs: changed(VALUE)}", file)?
  assert "{...base, build.jobs: changed(2)}" in updated, updated
  assert "{\"build.jobs\": 2}" in updated, updated
}

test test_grep_try_capture_matches_boundary_and_nested_call { |ctx|
  let file = script(ctx, "try-capture.xsh", "let result = try { load(\"α\")? }\n")?
  for pattern in ["try { EXPR }", "load(ARG)"] {
    let stdout = found(pattern, file)?
    assert "load(\"α\")" in stdout, stdout
  }
}

test test_refactor_try_capture_preserves_boundary_and_second_pass_is_empty { |ctx|
  let file = script(ctx, "refactor-try-capture.xsh", "let result = try { load(\"α\")? }\n")?
  let fixed = rewritten("load(ARG)", "read(ARG)", file)?
  assert "try { read(\"α\")? }" in fixed, fixed
  assert unchanged("load(ARG)", "read(ARG)", file)? == fixed
}

test test_grep_pattern_alternatives_preserve_order_and_nested_shapes { |ctx|
  let stdout = script(
    ctx,
    "pattern-alternatives.xsh",
    "let selected = 1 is (1 | 2)\nlet reversed = 1 is (2 | 1)\nlet nested = [1] is ([1] | [2])\n",
  )? |> found("SUBJECT is (1 | 2)", _)?
  assert "1 is (1 | 2)" in stdout, stdout
  assert "2 | 1" not in stdout, stdout
  assert "[1] | [2]" not in stdout, stdout
}

test test_grep_pattern_aliases_match_whole_subject_aliases_without_losing_precedence { |ctx|
  let stdout = script(
    ctx,
    "pattern-aliases.xsh",
    "let whole = match 1 { (1 | 2) as original => original _ => 0 }\nlet separate = match 1 { 1 as original | 2 as original => original _ => 0 }\n",
  )? |> found("match SUBJECT { (1 | 2) as original => BODY _ => 0 }", _)?
  assert "(1 | 2) as original" in stdout, stdout
  assert "1 as original | 2 as original" not in stdout, stdout
}

test test_refactor_pattern_alternatives_uses_original_subject_span_and_converges { |ctx|
  let file = script(
    ctx,
    "pattern-alternatives.xsh",
    "let value = 1 # café\nlet selected = value is (1 | 2)\nlet other = value is (2 | 3)\n",
  )?
  let fixed = rewritten("SUBJECT is (1 | 2)", "SUBJECT is (1 | 2 | 3)", file)?
  assert "value is (1 | 2 | 3)" in fixed, fixed
  assert "value is (2 | 3)" in fixed, fixed
  assert_checks(file)
  assert unchanged("SUBJECT is (1 | 2)", "SUBJECT is (1 | 2 | 3)", file)? == fixed
}

test test_grep_and_refactor_value_pipeline_holes_preserve_explicit_argument_placement { |ctx|
  let source = "let text = \"é\" |> render(\"[\", value: _)\n"
  let file = script(ctx, "pipeline-hole.xsh", source)?
  let stdout = found("INPUT |> render(PREFIX, value: _)", file)?
  assert "\"é\" |> render(\"[\", value: _)" in stdout, stdout
  let _ = absent("INPUT |> render(PREFIX, alternate: _)", file)?
  assert rewritten("INPUT |> render(PREFIX, value: _)", "INPUT |> render(PREFIX, value: _)", file)? == source
}

test test_refactor_value_pipeline_holes_retains_optional_calls_and_result_boundaries { |ctx|
  for {name, source, pattern} in [
    {
      name: "pipeline-result.xsh",
      source: "let value = \"3\" |> parse(_, radix: 10)?\n",
      pattern: "INPUT |> parse(_, radix: RADIX)?",
    },
    {
      name: "pipeline-optional.xsh",
      source: "let value = \"a\" |> maybe?.replace(_, \"b\")\n",
      pattern: "INPUT |> RECEIVER?.replace(_, OTHER)",
    },
  ] {
    let file = script(ctx, name, source)?
    let fixed = rewritten(pattern, pattern, file)?
    for hole in ["INPUT", "RADIX", "RECEIVER", "OTHER"] {
      assert hole not in fixed, fixed
    }

    assert_parses(file)
    for retained in ["|>", "_", "?"] {
      assert retained in fixed, fixed
    }
  }
}

test test_grep_and_refactor_visit_named_stream_configuration_and_spread_values { |ctx|
  let file = script(
    ctx,
    "stage-config.xsh",
    "let values = [1] |> par-map(jobs: worker_limit(1)) { |item| item } |> sort(...{desc: direction(true)}) # retain\n",
  )?
  for query in ["worker_limit(EXPR)", "direction(EXPR)"] {
    let stdout = found(query, file)?
    assert "1 match" in stdout, stdout
  }

  let fixed = rewritten("worker_limit(X)", "bounded_workers(X)", file)?
  assert "jobs: bounded_workers(1)" in fixed, fixed
  assert "sort(...{desc: direction(true)})" in fixed, fixed
  assert "# retain" in fixed, fixed
}

test test_grep_and_refactor_visit_static_stage_callable_descriptors { |ctx|
  let file = script(
    ctx,
    "stage-callable.xsh",
    "pure normalize(item: Str) -> Str { item.lower() }\npure canonicalize(item: Str) -> Str { item.upper() }\nlet values = [\"a\"] |> map(block: normalize) # retain descriptor comment\nprint values[0]\n",
  )?
  let stdout = found("normalize", file)?
  assert "1 match" in stdout, stdout
  let fixed = rewritten("normalize", "canonicalize", file)?
  assert "map(block: canonicalize)" in fixed, fixed
  assert "# retain descriptor comment" in fixed, fixed
}

test test_grep_assert_finds_condition_and_message_calls { |ctx|
  let stdout = script(
    ctx,
    "assert.xsh",
    "assert observed(1) == observed(2), observed(3)\n",
  )? |> found("observed(EXPR)", _)?
  for call in ["observed(1)", "observed(2)", "observed(3)", "3 matches"] {
    assert call in stdout, stdout
  }
}

test test_grep_and_refactor_selective_retry_preserve_filter_order_and_body { |ctx|
  let file = script(
    ctx,
    "selective-retry.xsh",
    "let result = retry [0ms] on (FetchError.Busy | FetchError.Timeout) { fetch(\"café\")? }\nlet all = retry [0ms] { fetch(\"all\")? }\n",
  )?
  let pattern = "retry [DELAY] on (FetchError.Busy | FetchError.Timeout) { fetch(ARG)? }"
  let stdout = found(pattern, file)?
  assert "1 match" in stdout, stdout
  let fixed = rewritten(pattern, "retry [DELAY] on (FetchError.Busy | FetchError.Timeout) { load(ARG)? }", file)?
  assert "{ load(\"café\")? }" in fixed, fixed
  assert "retry [0ms] { fetch(\"all\")? }" in fixed, fixed
  assert_parses(file)
}

test test_grep_duration_arithmetic_preserves_units_and_operand_order { |ctx|
  let stdout = script(
    ctx,
    "duration-arithmetic.xsh",
    "let first = 250ms * 3\nlet second = 3 * 250ms\nlet other = 250s * 3\n",
  )? |> found("250ms * COUNT", _)?
  assert "250ms * 3" in stdout, stdout
  assert "3 * 250ms" not in stdout, stdout
  assert "250s * 3" not in stdout, stdout
}

test test_grep_and_refactor_visit_named_spread_operands { |ctx|
  let file = script(
    ctx,
    "named-spread-operand.xsh",
    r"""pure sum(first: Int, second: Int) -> Int { first + second }
pure options() -> Pair { Pair(first: 2, second: 3) }
type Pair = {first: Int, second: Int}
print ${sum(...options())}
""",
  )?
  let stdout = found("options()", file)?
  assert "options()" in stdout, stdout
  let fixed = rewritten("options()", "Pair(first: 4, second: 5)", file)?
  assert "sum(...Pair(first: 4, second: 5))" in fixed, fixed
}

test test_grep_and_refactor_named_argument_spreads_retain_splice_and_label_identity { |ctx|
  let file = script(
    ctx,
    "spread-argument-identity.xsh",
    "let options = {first: 1}\nf(...options)\nf(@options)\nf(options)\nf(first: options)\n",
  )?
  let stdout = found("f(...EXPR)", file)?
  assert "f(...options)" in stdout, stdout
  assert "f(@options)" not in stdout, stdout
  assert "f(first: options)" not in stdout, stdout
  let fixed = rewritten("f(...EXPR)", "g(...EXPR)", file)?
  assert "g(...options)" in fixed, fixed
  assert "f(@options)" in fixed, fixed
  assert "f(first: options)" in fixed, fixed
}

test test_grep_and_refactor_lexical_ctx_preserve_description_and_body { |ctx|
  let file = script(ctx, "context.xsh", "let selected = ctx \"café\" { 7 }\nlet other = ctx \"other\" { 9 }\n")?
  let stdout = found("ctx \"café\" { BODY }", file)?
  assert "ctx \"café\" { 7 }" in stdout, stdout
  assert "other" not in stdout, stdout
  let fixed = rewritten("ctx \"café\" { BODY }", "ctx \"café updated\" { BODY }", file)?
  assert "ctx \"café updated\" { 7 }" in fixed, fixed
  assert unchanged("ctx \"café\" { BODY }", "ctx \"café updated\" { BODY }", file)? == fixed
}

test test_grep_and_refactor_typed_map_keys_preserve_computed_domains { |ctx|
  let file = script(
    ctx,
    "typed-map.xsh",
    "var entries: Map[Int, Str] = {[20]: \"twenty\", [3]: \"three\"}\n",
  )?
  let stdout = found("{[KEY]: VALUE, [OTHER]: REST}", file)?
  assert "[20]" in stdout, stdout
  let updated = rewritten("{[KEY]: VALUE, [OTHER]: REST}", "{[OTHER]: REST, [KEY]: VALUE}", file)?
  assert "Map[Int, Str]" in updated, updated
  assert "{[3]: \"three\", [20]: \"twenty\"}" in updated, updated
}

test test_grep_and_refactor_scalar_iteration_keep_loop_and_comprehension_sources { |ctx|
  let file = script(
    ctx,
    "scalar-iteration.xsh",
    r"""# Unicode loop
for character in "é" { print $character }
let octets = [octet for octet in b"\x00\xff"]
""",
  )?
  let stdout = found("\"é\"", file)?
  assert "1 match" in stdout, stdout
  let fixed = rewritten("\"é\"", "\"🙂\"", file)?
  assert "# Unicode loop\nfor character in \"🙂\"" in fixed, fixed
  assert "[octet for octet in b\"\\x00\\xff\"]" in fixed, fixed
  assert_checks(file)
  assert unchanged("\"é\"", "\"🙂\"", file)? == fixed
}

test test_grep_and_refactor_typed_causes_keep_named_operand { |ctx|
  let file = script(
    ctx,
    "typed-cause.xsh",
    "error Outer = Failed(message: Str)\nerror Inner = Failed(message: Str)\nlet value = Err(Outer.Failed(message: \"outer\"), cause: Inner.Failed(message: \"inner\"))\n",
  )?
  let stdout = found("Err(OUTER, cause: CAUSE)", file)?
  assert "cause: Inner.Failed" in stdout, stdout
  let updated = rewritten("Inner.Failed(message: VALUE)", "Inner.Failed(message: VALUE)", file)?
  assert "cause: Inner.Failed(message: \"inner\")" in updated, updated
}

test test_grep_and_refactor_reach_accept_policy_expressions { |ctx|
  let file = script(
    ctx,
    "accept.xsh",
    "pure choose_codes(codes: List[Int]) -> List[Int] { codes }\nrun.status --accept=choose_codes([0,1]) sh\n",
  )?
  let _ = found("choose_codes(EXPR)", file)?
  let updated = rewritten("choose_codes(EXPR)", "EXPR", file)?
  assert "--accept=[0,1] sh" in updated, updated
  let _ = absent("choose_codes(EXPR)", file)?
}

test test_grep_and_refactor_reach_wire_enum_mapping_expressions { |ctx|
  let file = script(
    ctx,
    "state.xsh",
    "enum State: Str { Ready = \"rea\" + \"dy\", Empty = \"\" }\nlet state: State = Ready\nprint json.encode(state)?\n",
  )?
  let stdout = found("LEFT + RIGHT", file)?
  assert "1 match" in stdout, stdout
  let fixed = rewritten("\"rea\" + \"dy\"", "\"ready\"", file)?
  assert "Ready = \"ready\"" in fixed, fixed
  assert "Empty = \"\"" in fixed, fixed
  assert_checks(file)
}

test test_grep_and_refactor_context_scopes_preserve_input_body_and_scope_kind { |ctx|
  let file = script(ctx, "scope.xsh", "let selected = env ({X: 7}) { 9 }\nlet other = cd (p\".\") { 9 }\n")?
  let stdout = found("env (EXPR) { BODY }", file)?
  assert "env ({X: 7}) { 9 }" in stdout, stdout
  assert "cd (" not in stdout, stdout
  let fixed = rewritten("env (EXPR) { BODY }", "env (EXPR) { BODY }", file)?
  assert "env ({X: 7}) {" in fixed, fixed
  assert "cd (p\".\") {" in fixed, fixed
}

test test_grep_matches_a_resource_scope_in_statement_and_value_position { |ctx|
  let file = script(
    ctx,
    "resource.xsh",
    "proc f(dir: Path) [fs, error] {\n  with root = fs.open_root(dir)? { root.close() }\n  let n = with held = fs.lock(dir)? { 1 }?\n  print $n\n}\n",
  )?
  let opened = found("with NAME = fs.open_root(EXPR)? { BODY }", file)?
  assert "with root = fs.open_root(dir)? { root.close() }" in opened, opened
  assert "fs.lock" not in opened, opened
  let named = found("with held = EXPR { BODY }", file)?
  assert "with held = fs.lock(dir)? { 1 }" in named, named
  assert "fs.open_root" not in named, named
  let other = absent("with other = EXPR { BODY }", file)?
  assert "0 matches" in other, other
}
