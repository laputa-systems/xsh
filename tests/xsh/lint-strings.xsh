type Captured = {status: Status, stdout: Str, stderr: Str}

const newline_rule = "lint.redundant-newline-triple-string"
const dollar_rule = "lint.dollar-in-expression-string"

# Writes `source` to a fresh script file.
proc script(ctx: TestContext, source: Str) [fs, error] -> Result[Path] {
  test.temp_file(ctx, name: "strings.xsh", contents: bytes.from_text(source))
}

# Runs `xsht lint` with `arguments` on `file`.
proc lint(file: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  run.capture --text "xsht" lint @arguments $file
}

# Requires `file` to check without a diagnostic.
proc assert_checks(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

# Requires `file` to be laid out as `xsht fmt` prints it.
proc assert_formatted(file: Path) [process, env, error] {
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr
}

test test_single_newline_triple_string_fix_writes_the_escaped_newline { |ctx|
  let source = "let newline = \"\"\"\n\n\n\"\"\"\n\nlet sample = \"\"\"alpha\nbeta\"\"\"\n"
  let file = script(ctx, source)?
  assert_checks(file)
  let reported = lint(file, ["--only", newline_rule])?
  assert reported.status.exited_with(1), reported.stderr
  # Only the string that holds one newline is reported.
  assert reported.stderr.split(f"warn[{newline_rule}]").len() == 2, reported.stderr
  assert ":1:15\n" in reported.stderr, reported.stderr
  assert "help: replace with `\"\\n\"` -> \"\\n\"\n" in reported.stderr, reported.stderr

  let fixed = lint(file, ["--fix", "--only", newline_rule])?
  assert fixed.status.exited_with(0), fixed.stderr
  assert file.read_text()? == "let newline = \"\\n\"\n\nlet sample = \"\"\"alpha\nbeta\"\"\"\n"
  assert_checks(file)
  assert_formatted(file)
}

test test_formatter_preserves_single_newline_triple_string_lint_fix { |ctx|
  let file = script(ctx, "let newline = \"\"\"\n\n\n\"\"\"\n")?
  let reported = lint(file, ["--only", newline_rule])?
  assert f"warn[{newline_rule}]" in reported.stderr, reported.stderr
  let fixed = lint(file, ["--fix", "--only", newline_rule])?
  assert fixed.status.exited_with(0), fixed.stderr
  let escaped = "let newline = \"\\n\"\n"
  assert file.read_text()? == escaped
  assert_formatted(file)
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  assert file.read_text()? == escaped
}

test test_dollar_lookalike_in_expression_string_is_reported { |ctx|
  let file = script(
    ctx,
    r"""let body = "hello"
let line = "tags: $body"
print $line
""",
  )?
  assert_checks(file)
  assert_formatted(file)
  let reported = lint(file, ["--only", dollar_rule])?
  assert reported.status.exited_with(1), reported.stderr
  assert reported.stderr.split(f"warn[{dollar_rule}]").len() == 2, reported.stderr
  assert r"`$body` is literal text" in reported.stderr, reported.stderr
  assert ":2:19\n" in reported.stderr, reported.stderr
  assert "to interpolate `body`" in reported.stderr, reported.stderr
}

# Command-word interpolation, escaped dollars, raw strings, and f-strings
# must not warn.
test test_dollar_lint_skips_interpolating_strings_and_literal_dollar_contexts { |ctx|
  let file = script(
    ctx,
    r"""let body = "hello"
let escaped = "literal \$body"
let raw = r"$body"
let fmt = f"tags: {body}"
print "tags: $body" $escaped $raw $fmt
""",
  )?
  assert_checks(file)
  assert_formatted(file)
  let reported = lint(file, ["--only", dollar_rule])?
  assert reported.status.exited_with(0), reported.stderr
  assert dollar_rule not in reported.stderr, reported.stderr
}

# Dollar lookalikes that do not name a binding should not warn.
test test_dollar_lint_skips_unbound_lookalikes_in_expression_string { |ctx|
  let file = script(
    ctx,
    r"""let note = "home: $HOME cost: $5 template: $unbound and $field.field"
print $note
""",
  )?
  assert_checks(file)
  assert_formatted(file)
  let reported = lint(file, ["--only", dollar_rule])?
  assert reported.status.exited_with(0), reported.stderr
  assert dollar_rule not in reported.stderr, reported.stderr
}

test test_dollar_lookalike_in_triple_quoted_and_parenthesized_expressions_is_reported { |ctx|
  let file = script(
    ctx,
    "let body = \"hello\"\nlet block = \"\"\"line one\ntags: \$body\nline three\"\"\"\nprint (\"tags: \$body\") \$block\n",
  )?
  assert_checks(file)
  let reported = lint(file, ["--only", dollar_rule])?
  assert reported.status.exited_with(1), reported.stderr
  let findings = reported.stderr.split(f"warn[{dollar_rule}]")
  assert findings.len() == 3, reported.stderr
  for finding in [findings[1], findings[2]] {
    assert r"`$body` is literal text" in finding, finding
  }

  assert ":3:7\n" in findings[1], reported.stderr
  assert ":5:15\n" in findings[2], reported.stderr
}
