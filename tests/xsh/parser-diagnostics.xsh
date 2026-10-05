# What the lexer and parser reject, as `xsht check` reports it: the diagnostic
# code, its message, and the fix it offers.

# What `xsht check` printed about a script holding `source` that it rejected.
proc rejection(ctx: TestContext, source: Str) [fs, process, error] -> Result[Str] {
  let file = test.temp_file(ctx, name: "rejected.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(2), f"{source}: {checked.stderr}"
  Ok(checked.stderr)
}

# Requires `xsht check` to report `code` as an error for `source`.
proc expect_rejected(ctx: TestContext, source: Str, code: Str) [fs, process, error] {
  let stderr = rejection(ctx, source)?
  assert f"err[{code}]" in stderr, f"{source}: {stderr}"
}

# Requires `source` to get past the lexer and the parser. The checker may
# still reject it: these scripts name things they do not declare.
proc expect_parses(ctx: TestContext, source: Str) [fs, process, error] {
  let file = test.temp_file(ctx, name: "parsed.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  assert "[parse." not in checked.stderr, f"{source}: {checked.stderr}"
  assert "[lex." not in checked.stderr, f"{source}: {checked.stderr}"
}

test test_bytes_literal_rejects_a_unicode_escape { |ctx|
  expect_rejected(ctx, "let data = b\"\\u{41}\"\n", "lex.invalid-bytes-escape")
}

# Unsupported C-style boolean operators and the `then` keyword must be
# named by a constructive diagnostic that points at the offending token,
# not at the block brace that follows the condition.
test test_c_style_boolean_operators_and_then_are_named { |ctx|
  let cases = [
    {source: "proc main() { if a || b { } }\n", code: "parse.unsupported-boolean-operator", token: "^^"},
    {source: "proc main() { if a && b { } }\n", code: "parse.unsupported-boolean-operator", token: "^^"},
    # A doubled operator is reported whether or not its halves touch.
    {source: "proc main() { if a | | b { } }\n", code: "parse.unsupported-boolean-operator", token: "^^^"},
    {source: "proc main() { if a & & b { } }\n", code: "parse.unsupported-boolean-operator", token: "^^^"},
    {source: "proc main() { if a then { } }\n", code: "parse.unsupported-then", token: "^^^^"},
  ]
  for {source, code, token} in cases {
    let stderr = rejection(ctx, source)?
    assert f"err[{code}]" in stderr, f"{source}: {stderr}"
    # The caret line starts under column 20, where the operator is.
    assert f":1:20\n  {source}                     {token} " in stderr, f"{source}: {stderr}"
  }
}

# The valid `or`/`and` word forms must parse without diagnostics so the
# constructive error does not change valid-program behavior.
test test_word_form_boolean_operators_parse { |ctx|
  for source in ["proc main() { if a or b { } }\n", "proc main() { if a or b and c { } }\n"] {
    expect_parses(ctx, source)
  }
}

test test_integer_division_spellings_point_at_slash { |ctx|
  for source in ["let quotient = 7 // 2\n", "let quotient = 7 div 2\n"] {
    let stderr = rejection(ctx, source)?
    assert "err[parse.unsupported-integer-division]" in stderr, stderr
    assert "use `/` on Int operands" in stderr, stderr
    assert "help: replace with integer `/` -> /\n" in stderr, stderr
  }

  test.expect(ctx, "let quotient = 7 / 2\nprint \$quotient\n", status: 0, stdout: ["3"])?
}

test test_signal_hook_requires_effects_and_a_duration_option { |ctx|
  expect_rejected(ctx, "on SIGINT {\n}\n", "parse.signal-hook")
  expect_rejected(ctx, "on TERM --pre-cancel=soon [] {\n}\n", "parse.signal-hook")
}

test test_stale_surface_syntax_is_rejected { |ctx|
  for source in ["let label = fmt\"hello\"\n", "let files = glob\"*.rs\"\n", "let ok = not ready\n"] {
    let stderr = rejection(ctx, source)?
    assert "err[parse." in stderr, f"{source}: {stderr}"
  }
}

# The removed schema helper has no special syntax left: its name parses as a
# call, and only name resolution rejects it.
test test_old_schema_helper_name_is_a_plain_call { |ctx|
  let stderr = rejection(ctx, "type Row = {name: Str}\nlet raw = {name: \"demo\"}\nlet row = validate(raw, Row)?\n")?
  assert "err[check.unresolved-call]: unresolved pure function call `validate`" in stderr, stderr
  assert "[parse." not in stderr, stderr
}

test test_proc_requires_a_signature { |ctx|
  expect_rejected(ctx, "proc build {\n  print \"bad\"\n}\n", "parse.required-signature")
}

test test_keywords_and_hyphenated_names_are_not_binding_names { |ctx|
  expect_rejected(ctx, "let if = 1\n", "parse.expected-ident")
  expect_rejected(ctx, "let build-all = 1\n", "parse.expected-ident")
}

test test_colon_inclusive_and_stride_slices_are_rejected { |ctx|
  for source in ["let part = b\"abcd\"[0:2]\n", "let part = b\"abcd\"[..=2]\n", "let part = b\"abcd\"[0..2..1]\n"] {
    let stderr = rejection(ctx, source)?
    assert "err[parse." in stderr, f"{source}: {stderr}"
  }
}

test test_ordering_and_pattern_tests_need_grouping { |ctx|
  for source in ["let result = 0 < 1 < 2 is Bool\n", "let result = value is Str < true\n"] {
    expect_rejected(ctx, source, "parse.mixed-comparison")
  }

  for source in ["let result = (0 < 1 < 2) is Bool\n", "let result = (value is Str) < true\n"] {
    expect_parses(ctx, source)
  }
}

test test_unterminated_regex_literals_are_lexical_errors { |ctx|
  for source in ["let pattern = rx\"abc", "let pattern = rx\"\"\"abc\n"] {
    expect_rejected(ctx, source, "lex.unterminated-string")
  }
}

test test_keyword_field_labels_cannot_be_puns_or_binding_names { |ctx|
  for source in [
    "let row = {type}\n",
    "let {type} = {type: 1}\n",
    "type Entry = {type: Int}\nlet entry = Entry(type:)\n",
    "let row = {type: 1}\nlet selected = match row { {type} => 1, _ => 2 }\n",
  ] {
    expect_rejected(ctx, source, "parse.keyword-label-binding")
  }

  for source in ["let type = 1\n", "pure value(match: Int) -> Int { 1 }\n", "use fs as type\n"] {
    expect_rejected(ctx, source, "parse.expected-ident")
  }
}

test test_value_pipeline_hole_must_be_one_whole_argument { |ctx|
  for source in [
    "1 |> render(_, _)\n",
    "1 |> render(_ + 1)\n",
    "1 |> render(nested(_))\n",
    "1 |> render(@_)\n",
    "1 |> render(if true { _ } else { 0 })\n",
  ] {
    expect_rejected(ctx, source, "parse.pipeline-hole")
  }
}

# A stage flag followed by a parenthesized argument could be rewritten two
# ways, so the migration diagnostic offers no fix. Flags of an external
# command are ordinary words.
test test_stream_stage_flag_migration_offers_no_fix_when_ambiguous { |ctx|
  let stderr = rejection(ctx, "let values = [1] |> sort-by --desc (.size)\n")?
  assert "err[parse.stream-option-migration]" in stderr, stderr
  assert "help:" not in stderr, stderr
  expect_parses(ctx, "run printf --jobs --desc --max-bytes\n")
}

test test_selective_retry_requires_a_parenthesized_clause { |ctx|
  expect_rejected(ctx, "let result = retry [] on FetchError.Busy { fetch()? }", "parse.expected-token")
  # `on` stays an ordinary name and command word.
  test.expect(ctx, "let on = 1\nlet result = retry [] { on }\nrun echo on\n", status: 0, stdout: ["on"])?
}

# A punned argument `value:` reads the lexical name `value`; when there is
# none, the diagnostic points at the identifier, counted in characters after
# the multibyte comment above it.
test test_named_argument_pun_reports_the_missing_name_at_the_identifier {
  let checked = run.capture --text "xsht" check tests/fixtures/sema/named-argument-pun-missing.xsh
  assert checked.status.exited_with(2), checked.stderr
  assert "err[check.unresolved-name]: unresolved name `value`" in checked.stderr, checked.stderr
  assert "named-argument-pun-missing.xsh:4:21\n  let result = accept(value:)\n                      ^^^^^ unresolved name\n" in checked.stderr, checked.stderr
}

# Each stage with flags is its own fatal diagnostic, and applying the fixes
# rewrites a valued flag, a bare flag, and a flag whose value is an expression
# without touching the comments around them.
test test_stream_stage_flag_migration_fix_rewrites_every_stage { |ctx|
  let source = "# café\nlet workers = 2\nlet values = [1] |> par-map --jobs=workers { |item| item } # retain\nlet groups = values |> reduce-by --sum --jobs=2 { |item| {key: \"all\", value: item} }\n"
  let stderr = rejection(ctx, source)?
  assert stderr.split("err[").len() == 3, stderr
  assert stderr.split("err[parse.stream-option-migration]").len() == 3, stderr

  let file = test.temp_file(ctx, name: "stage-flags.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --fix $file
  assert applied.status.exited_with(0), applied.stderr
  let fixed = file.read_text()?
  assert "par-map (jobs: workers) { |item| item } # retain" in fixed, fixed
  assert "reduce-by (sum: true, jobs: 2)" in fixed, fixed
  assert fixed.starts_with("# café\n"), fixed
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}
