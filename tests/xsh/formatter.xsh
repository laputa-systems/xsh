proc assert_fmt_fixture(ctx: TestContext, source_path: Path, expected_path: Path, name: Str) [fs, process, error] {
  let source = source_path.read_text()?
  let expected = expected_path.read_text()?
  let candidate = test.temp_file(ctx, name:, contents: bytes.from_text(source))?

  let formatted = run.capture --text "xsht" fmt $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  assert candidate.read_text()? == expected

  let checked = run.capture --text "xsht" check $candidate ?
  assert checked.status.exited_with(0), checked.stderr

  let stable = run.capture --text "xsht" fmt --check $candidate ?
  assert stable.status.exited_with(0), stable.stderr
}

test test_fmt_fixture { |ctx|
  assert_fmt_fixture(
    ctx,
    p"tests/fixtures/fmt/beauty.xsh",
    p"tests/fixtures/fmt/beauty.expected.xsh",
    "fmt-beauty.xsh",
  )?
}

test test_fmt_nested_multiline_string_preserves_value { |ctx|
  let source = p"tests/fixtures/fmt/nested-multiline-string.xsh".read_text()?
  let before = test.run_script(ctx, source)?
  assert before.success, before.stderr
  let candidate = test.temp_file(ctx, name: "nested-string.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  let after = test.run_script(ctx, candidate.read_text()?)?
  assert after.success, after.stderr
  assert after.stdout == before.stdout
  let stable = run.capture --text "xsht" fmt --check $candidate ?
  assert stable.status.exited_with(0), stable.stderr
}

test test_fmt_path_format_specs_preserve_value { |ctx|
  let source = p"tests/fixtures/fmt/path-format-specs.xsh".read_text()?
  let before = test.run_script(ctx, source)?
  assert before.success, before.stderr
  let candidate = test.temp_file(ctx, name: "path-format-specs.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  let after = test.run_script(ctx, candidate.read_text()?)?
  assert after.success, after.stderr
  assert after.stdout == before.stdout
  let stable = run.capture --text "xsht" fmt --check $candidate ?
  assert stable.status.exited_with(0), stable.stderr
}

test test_fmt_multiline_comprehension_pipelines_preserve_value { |ctx|
  let source = p"tests/fixtures/fmt/comprehension-pipelines.xsh".read_text()?
  let before = test.run_script(ctx, source)?
  assert before.success, before.stderr
  let candidate = test.temp_file(ctx, name: "comprehension-pipelines.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  let after = test.run_script(ctx, candidate.read_text()?)?
  assert after.success, after.stderr
  assert after.stdout == before.stdout
  let stable = run.capture --text "xsht" fmt --check $candidate ?
  assert stable.status.exited_with(0), stable.stderr
}

test test_fmt_match_arm_nested_blocks_preserve_value { |ctx|
  let source = p"tests/fixtures/fmt/match-arm-nested-blocks.xsh".read_text()?
  let before = test.run_script(ctx, source)?
  assert before.success, before.stderr
  let candidate = test.temp_file(ctx, name: "match-arm-nested-blocks.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
  let after = test.run_script(ctx, candidate.read_text()?)?
  assert after.success, after.stderr
  assert after.stdout == before.stdout
  let stable = run.capture --text "xsht" fmt --check $candidate ?
  assert stable.status.exited_with(0), stable.stderr
}
