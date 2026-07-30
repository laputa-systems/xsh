proc assert_fmt_fixture(
  ctx: TestContext,
  source_path: Path,
  expected_path: Path,
  name: Str,
) [fs, process, error] {
  let source = source_path.read_text()?
  let expected = expected_path.read_text()?
  let candidate = test.temp_file(ctx, name: name, contents: bytes.from_text(source))?

  let formatted = run.capture --text "xsht" fmt $candidate ?
  test.ok(formatted.status.exited_with(0), formatted.stderr)?
  test.eq(candidate.read_text()?, expected)?

  let checked = run.capture --text "xsht" check $candidate ?
  test.ok(checked.status.exited_with(0), checked.stderr)?

  let stable = run.capture --text "xsht" fmt --check $candidate ?
  test.ok(stable.status.exited_with(0), stable.stderr)?
}

proc test_fmt_fixture(ctx: TestContext) [fs, process, error] {
  assert_fmt_fixture(
    ctx,
    p"tests/fixtures/fmt/beauty.xsh",
    p"tests/fixtures/fmt/beauty.expected.xsh",
    "fmt-beauty.xsh",
  )?
}
