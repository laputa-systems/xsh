proc assert_fmt_fixture(ctx: TestContext, source_path: Path, expected_path: Path, name: Str) [fs, process, error] {
  let source = source_path.read_text()?
  let expected = expected_path.read_text()?
  let candidate = test.temp_file(ctx, name:, contents: bytes.from_text(source))?

  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  assert candidate.read_text()? == expected

  let checked = run.capture --text "xsht" check $candidate
  assert checked.status.exited_with(0), checked.stderr

  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
}

test test_fmt_fixture { |ctx|
  assert_fmt_fixture(
    ctx,
    p"tests/fixtures/fmt/beauty.xsh",
    p"tests/fixtures/fmt/beauty.expected.xsh",
    "fmt-beauty.xsh",
  )
}

test test_fmt_env_strings_round_trip { |ctx|
  assert_fmt_fixture(
    ctx,
    p"tests/fixtures/fmt/env-strings.xsh",
    p"tests/fixtures/fmt/env-strings.expected.xsh",
    "fmt-env-strings.xsh",
  )
}

test test_fmt_target_typed_variants_and_positional_constructors { |ctx|
  let source = p"tests/fixtures/fmt/target-typed-constructors.xsh".read_text()?
  let before = test.expect(ctx, source, status: 0)?
  assert_fmt_fixture(
    ctx,
    p"tests/fixtures/fmt/target-typed-constructors.xsh",
    p"tests/fixtures/fmt/target-typed-constructors.expected.xsh",
    "fmt-target-typed.xsh",
  )
  let after = test.expect(ctx, p"tests/fixtures/fmt/target-typed-constructors.expected.xsh".read_text()?, status: 0)?
  assert after.stdout == before.stdout
}

test test_fmt_nested_multiline_string_preserves_value { |ctx|
  let source = p"tests/fixtures/fmt/nested-multiline-string.xsh".read_text()?
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "nested-string.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  let after = test.expect(ctx, candidate.read_text()?, status: 0)?
  assert after.stdout == before.stdout
  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
}

# A conditional or comprehension on the right of an assignment breaks the way
# a `let` initializer does, and a conditional operand never breaks inside a
# one-line branch, so a second pass changes nothing.
test test_fmt_assigned_conditionals_are_stable { |ctx|
  let source = p"tests/fixtures/fmt/assigned-conditionals.xsh".read_text()?
  let before = test.expect(ctx, source, status: 0)?
  assert_fmt_fixture(
    ctx,
    p"tests/fixtures/fmt/assigned-conditionals.xsh",
    p"tests/fixtures/fmt/assigned-conditionals.expected.xsh",
    "assigned-conditionals.xsh",
  )
  let after = test.expect(ctx, p"tests/fixtures/fmt/assigned-conditionals.expected.xsh".read_text()?, status: 0)?
  assert after.stdout == before.stdout
}

# A continued command keeps the author's line breaks, a command too long for
# its line is broken with `\`, and neither changes what the commands run.
test test_fmt_command_continuation { |ctx|
  let before = test.expect(ctx, p"tests/fixtures/fmt/command-continuation.xsh".read_text()?, status: 0)?
  assert_fmt_fixture(
    ctx,
    p"tests/fixtures/fmt/command-continuation.xsh",
    p"tests/fixtures/fmt/command-continuation.expected.xsh",
    "command-continuation.xsh",
  )
  let after = test.expect(ctx, p"tests/fixtures/fmt/command-continuation.expected.xsh".read_text()?, status: 0)?
  assert after.stdout == before.stdout
}

# A comment that ends a command's line stays on that line, as it does after
# any other statement; it used to move to a line of its own below.
test test_fmt_keeps_a_trailing_comment_on_a_command { |ctx|
  let source = "print one two # said\nrun true # ran\nlet text = run.text printf x ? # captured\nprint $text \\\n  again # continued\n\nif true {\n  print inside # nested\n}\n"
  let file = test.temp_file(ctx, name: "trailing.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  assert file.read_text()? == source
}

test test_fmt_path_format_specs_preserve_value { |ctx|
  let source = p"tests/fixtures/fmt/path-format-specs.xsh".read_text()?
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "path-format-specs.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  let after = test.expect(ctx, candidate.read_text()?, status: 0)?
  assert after.stdout == before.stdout
  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
}

test test_fmt_multiline_comprehension_pipelines_preserve_value { |ctx|
  let source = p"tests/fixtures/fmt/comprehension-pipelines.xsh".read_text()?
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "comprehension-pipelines.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  let after = test.expect(ctx, candidate.read_text()?, status: 0)?
  assert after.stdout == before.stdout
  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
}

test test_fmt_match_arm_nested_blocks_preserve_value { |ctx|
  let source = p"tests/fixtures/fmt/match-arm-nested-blocks.xsh".read_text()?
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "match-arm-nested-blocks.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  let after = test.expect(ctx, candidate.read_text()?, status: 0)?
  assert after.stdout == before.stdout
  let stable = run.capture --text "xsht" fmt --check $candidate
  assert stable.status.exited_with(0), stable.stderr
}

test test_fmt_error_families_choose_the_form_by_width { |ctx|
  let source = p"tests/fixtures/fmt/error-families.xsh".read_text()?
  let before = test.expect(ctx, source, status: 0)?
  assert_fmt_fixture(
    ctx,
    p"tests/fixtures/fmt/error-families.xsh",
    p"tests/fixtures/fmt/error-families.expected.xsh",
    "fmt-error-families.xsh",
  )
  let after = test.expect(ctx, p"tests/fixtures/fmt/error-families.expected.xsh".read_text()?, status: 0)?
  assert after.stdout == before.stdout
}

test test_fmt_keeps_the_dollar_escapes_an_ordinary_string_was_written_with { |ctx|
  # `$name` in an ordinary string is literal text that
  # `lint.dollar-in-expression-string` questions, and `\$name` is how an author
  # answers it. Formatting keeps each spelling, so the value and the lint's
  # findings stay the same.
  let source = r"""let split = "x"
let name = "y"

proc show() {
  let nested = "env X=\$split {\n  print \"entered\"\n}\n"
  let mixed = "kept \$name, flagged $name, hex \x24name, price \$5"
  print $nested $mixed
}

show()
"""
  let expected = source.replace(r"hex \x24name, price \$5", with: r"hex \$name, price $5")
  let before = test.expect(ctx, source, status: 0)?
  let candidate = test.temp_file(ctx, name: "dollar-escapes.xsh", contents: bytes.from_text(source))?
  let flagged = run.capture --text "xsht" lint --only lint.dollar-in-expression-string $candidate
  assert flagged.stderr.split("warn[lint.dollar-in-expression-string]").len() == 2, flagged.stderr
  let formatted = run.capture --text "xsht" fmt $candidate
  assert formatted.status.exited_with(0), formatted.stderr
  assert candidate.read_text()? == expected
  let after = test.expect(ctx, expected, status: 0)?
  assert after.stdout == before.stdout
  let still_flagged = run.capture --text "xsht" lint --only lint.dollar-in-expression-string $candidate
  assert still_flagged.stderr.split("warn[lint.dollar-in-expression-string]").len() == 2, still_flagged.stderr
  assert "flagged $name" in still_flagged.stderr, still_flagged.stderr
}
