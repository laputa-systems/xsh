# Forms the parser accepts, and how it groups them: where a line break
# continues an expression, where it ends a statement, and which spellings stay
# ordinary names.

# What `xsht check` printed about a script holding `source`.
proc check_output(ctx: TestContext, source: Str) [fs, process, error] -> Result[Str] {
  let file = test.temp_file(ctx, name: "form.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  Ok(checked.stderr)
}

# Requires `source` to parse and check with no diagnostic of any severity.
proc expect_checks(ctx: TestContext, source: Str) [fs, process, error] {
  let stderr = check_output(ctx, source)?
  assert rx"(?m)^(err|warn|note)\[".captures(stderr).is_empty(), f"{source}: {stderr}"
}

# Requires `source` to get past the lexer and the parser. The checker may
# still reject it: these scripts name things they do not declare.
proc expect_parses(ctx: TestContext, source: Str) [fs, process, error] {
  let stderr = check_output(ctx, source)?
  assert "[parse." not in stderr, f"{source}: {stderr}"
  assert "[lex." not in stderr, f"{source}: {stderr}"
}

test test_language_fixture_parses_with_comments_procs_and_pures { |ctx|
  let source = p"tests/fixtures/syntax/valid/language.xsh".read_text()?
  for fragment in ["# ", "proc ", "pure "] {
    assert fragment in source, fragment
  }

  expect_checks(ctx, source)
}

test test_markdown_headings_in_a_string_are_not_doc_comments { |ctx|
  expect_checks(ctx, "\nlet report = \"# Manager\\n\\n## North-star impact\\n\\nfixture\\n\\n## task-tags\\n\"\n")
}

test test_keywords_are_schema_and_record_field_labels { |ctx|
  expect_checks(ctx, "type Accum = {run: Int, lines: List[Str]}\nlet rec: Accum = {run: 0, lines: []}\n")
}

test test_quoted_reserved_words_are_record_fields { |ctx|
  expect_parses(ctx, "let rec = {\"run\": 0, \"lines\": []}\n")
}

# A brace or quote inside a nested raw, block, or formatted string does not
# end the interpolation that holds it.
test test_nested_interpolation_boundaries { |ctx|
  expect_parses(
    ctx,
    "\nlet label = f\"{ {raw: r\"}\", triple: \"\"\"}\"\"\", nested: f\"{ {brace: \"}\"} .brace }\"}.nested }\"\nrun echo \"\${{name: f\"{1}\", text: \"}\"} .name}\"\n",
  )
}

test test_pipeline_value_calls_take_plain_receivers_result_tails_and_named_blocks { |ctx|
  expect_checks(
    ctx,
    """
let parts = "a,b" |> split(",")
let selected = [{value: "b"}] |> where { |entry| entry.value == "b" } |> first()?
let first = ["a", "b"] |> get(0)?
""",
  )
}

test test_leading_operator_continues_the_previous_line { |ctx|
  let _ = test.expect(ctx, "let x = 1\n+ 2\nprint \$x\n", status: 0, stdout: ["3"])?
}

test test_trailing_operator_continues_onto_the_next_line { |ctx|
  let _ = test.expect(ctx, "let x = 1 +\n2\nprint \$x\n", status: 0, stdout: ["3"])?
}

test test_chained_comparisons_continue_across_newlines { |ctx|
  let _ = test.expect(
    ctx,
    "let x = 5\nlet y = 1\nlet ok = x > 0\nand x < 10\nand y != 0\nprint \$ok\n",
    status: 0,
    stdout: ["true"],
  )?
}

test test_newline_without_an_operator_ends_the_statement { |ctx|
  let _ = test.expect(ctx, "let x = 1\nlet y = 2\nprint \$x \$y\n", status: 0, stdout: ["1 2"])?
}

# A line that begins with a token that can start a statement is its own
# statement, never an operand of the line above: the checker reports exactly
# that line as an unused value.
test test_line_starting_with_a_statement_token_starts_a_new_statement { |ctx|
  let cases = [
    {line: "-1", carets: "^^", code: "check.ignored-result"},
    {line: "/tmp/x", carets: "^^^^^^", code: "check.ignored-result"},
    {line: "./x", carets: "^^^", code: "check.ignored-result"},
    {line: "is_ok(1)", carets: "^^^^^^^^", code: "check.bool-statement"},
  ]
  for {line, carets, code} in cases {
    let stderr = check_output(
      ctx,
      f"pure is_ok(n: Int) -> Bool {{ n == 1 }}\nlet value = 1\n{line}\nlet same: Int = value\n",
    )?
    assert "[parse." not in stderr, f"{line}: {stderr}"
    assert f"err[{code}]" in stderr, f"{line}: {stderr}"
    assert f":3:1\n  {line}\n  {carets} " in stderr, f"{line}: {stderr}"
    assert stderr.split("err[").len() == 2, f"{line}: {stderr}"
  }
}

test test_parenthesized_expression_spans_lines { |ctx|
  expect_parses(ctx, "let x = (1 +\n2)\n")
  let _ = test.expect(ctx, "let x = (1 +\n2) * 2\nprint \$x\n", status: 0, stdout: ["6"])?
}

test test_list_literal_spans_lines { |ctx|
  let _ = test.expect(ctx, "let xs = [\n1,\n2,\n3\n]\nprint \${xs.len()}\n", status: 0, stdout: ["3"])?
}

test test_record_literal_spans_lines { |ctx|
  let _ = test.expect(ctx, "let r = {\na: 1,\nb: 2,\n}\nprint \${r.a + r.b}\n", status: 0, stdout: ["3"])?
}

# The exhaustive match proves the declaration has exactly these variants.
test test_enum_declaration_spans_lines { |ctx|
  let _ = test.expect(
    ctx,
    r"""enum T {
  A,
 B,
 C(Int),
}
for value in [A, B, C(3)] {
  match value {
    A => print a
    B => print b
    C(n) => print $n
  }
}
""",
    status: 0,
    stdout: ["a\nb\n3"],
  )?
}

test test_enum_declaration_with_payload_variants_spans_lines { |ctx|
  let _ = test.expect(
    ctx,
    r"""enum Tok {
  TNum(Float),
 TStr(Str),
 TOp(Str),
 TEOF,
}
for token in [TNum(1.5), TStr("s"), TOp("+"), TEOF] {
  match token {
    TNum(number) => print $number
    TStr(text) => print $text
    TOp(operator) => print $operator
    TEOF => print eof
  }
}
""",
    status: 0,
    stdout: ["1.5\ns\n+\neof"],
  )?
}

# A one-variant `enum` is still a nominal type, and `type NAME = Other` with
# an identifier on the right stays an alias for it.
test test_singleton_enum_is_nominal_and_an_identifier_alias_names_it { |ctx|
  let _ = test.expect(
    ctx,
    r"""enum Token { Present(Str), }
type Alias = Token
let token: Alias = Present("x")
let same: Token = token
match same {
  Present(text) => print $text
}
""",
    status: 0,
    stdout: ["x"],
  )?
}

test test_plus_concatenates_strings { |ctx|
  let _ = test.expect(ctx, "let x = \"a\" + \"b\"\nprint \$x\n", status: 0, stdout: ["ab"])?
}

test test_plus_concatenates_a_chain_of_strings { |ctx|
  let _ = test.expect(ctx, "let x = \"a\" + \"b\" + \"c\"\nprint \$x\n", status: 0, stdout: ["abc"])?
}

test test_command_arguments_take_call_and_index_chains { |ctx|
  let _ = test.expect(
    ctx,
    "proc main() {\n  let c = {stderr: \"err\\n\"}\n  print c.stderr.trim()\n}\n\nmain()\n",
    status: 0,
    stdout: ["err"],
  )?
  let _ = test.expect(
    ctx,
    "proc main() {\n  let c = {stderr: \"err\\n\"}\n  print \${c.stderr.trim()}\n}\n\nmain()\n",
    status: 0,
    stdout: ["err"],
  )?
  expect_parses(ctx, "proc main() {\n  let x = \"hi\"\n  run.status x.trim()\n}\n")
  let grouped = check_output(ctx, "print (\"x\")\n")?
  assert "parse.command-call-expr" not in grouped, grouped
}

test test_type_pattern_arms_follow_unbraced_arm_values { |ctx|
  expect_parses(
    ctx,
    "pure describe(failure: Error) -> Str {\n  match failure {\n    is PermissionDenied => return \"permission_denied\"\n    is NotFound => return \"not_found\"\n    _ => return \"other\"\n  }\n}\n",
  )
}
