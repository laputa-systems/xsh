test test_prefer_fail_fix_preserves_nominal_failure { |ctx|
  let source = r"""error LoadError = Failed(message: Str)

proc load() -> Result[Int] {
  return Err(LoadError.Failed("no configuration"))
}

print load()?
"""
  let before = test.expect(ctx, source, status: 3)?
  assert "LoadError.Failed" in before.stderr, before.stderr
  let file = test.temp_file(ctx, name: "nominal.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text "xsht" lint --only lint.prefer-fail --fix $file
  assert fixed.status.exited_with(1), fixed.stderr
  assert "warn[lint.prefer-fail]" in fixed.stderr, fixed.stderr
  assert "help:" not in fixed.stderr, fixed.stderr
  assert file.read_text()? == source
  let after = test.expect(ctx, file.read_text()?, status: 3)?
  assert "LoadError.Failed" in after.stderr, after.stderr
  assert "no configuration" in after.stderr, after.stderr
}

test test_run_initializer_propagation_is_removed { |ctx|
  let source = r"""let inside = (run.text printf inside ?)
let outside = (run.text printf outside)?
var raw = (run.bytes printf first)?
raw = (run.bytes printf second ?)
print $inside $outside ${raw.len()}
"""
  let expected = r"""let inside = run.text printf inside
let outside = run.text printf outside
var raw = run.bytes printf first
raw = run.bytes printf second
print $inside $outside ${raw.len()}
"""
  let before = test.expect(ctx, source, status: 0)?
  let file = test.temp_file(ctx, name: "initializers.xsh", contents: bytes.from_text(source))?
  let listed = run.capture --text "xsht" lint --only lint.redundant-propagation $file
  assert listed.status.exited_with(1), listed.stderr
  assert listed.stderr.split("warn[lint.redundant-propagation]").len() == 5, listed.stderr
  let fixed = run.capture --text "xsht" lint --only lint.redundant-propagation --fix $file
  assert fixed.status.exited_with(0), fixed.stderr
  assert file.read_text()? == expected, file.read_text()?
  let after = test.expect(ctx, expected, status: 0)?
  assert after.stdout == before.stdout
}

test test_run_propagation_keeps_command_delimiters { |ctx|
  let source = r"""pure both(left: Str, right: Str) -> Str { left + right }
let joined = both("!", run.text printf a ?)
let listed = [run.text printf b ?]
assert run.text printf d? == "d"
let suffix = (run.text printf c)? + "!"
print $joined ${listed.len()} $suffix
"""
  let before = test.expect(ctx, source, status: 0)?
  let file = test.temp_file(ctx, name: "delimiters.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text "xsht" lint --only lint.redundant-propagation --fix $file
  assert fixed.status.exited_with(0), fixed.stderr
  assert file.read_text()? == source
  let after = test.expect(ctx, source, status: 0)?
  assert after.stdout == before.stdout
}

test test_prefer_fail_leading_dot_fix_preserves_failure_and_cause { |ctx|
  let source = r"""error LoadError = Failed | Other

proc load() -> Result[Int, LoadError] {
  defer { print "cleanup" }
  return Err(.Failed("outer"), cause: error.failure("inner"))
}

print load()?
"""
  let before = test.expect(ctx, source, status: 3)?
  let file = test.temp_file(ctx, name: "leading-dot.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text "xsht" lint --only lint.prefer-fail --fix $file
  assert fixed.status.exited_with(0), fixed.stderr
  let expected = source.replace(
    "return Err(.Failed(\"outer\"), cause: error.failure(\"inner\"))",
    with: "fail .Failed(\"outer\") because error.failure(\"inner\")",
  )
  assert file.read_text()? == expected
  let after = test.expect(ctx, expected, status: 3)?
  assert before.stdout == "cleanup\n" and after.stdout == before.stdout
  for report in [before.stderr, after.stderr] {
    assert "LoadError.Failed" in report and "outer" in report, report
    assert "caused by:" in report and "inner" in report, report
  }
}

test test_run_initializer_fix_preserves_command_failure { |ctx|
  let source = r"""proc load() -> Result[Str] {
  defer { print "cleanup" }
  let value = (run.text false)?
  print "unreachable"
  Ok(value)
}

print load()?
"""
  let before = test.expect(ctx, source, status: 3)?
  let file = test.temp_file(ctx, name: "run-failure.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text "xsht" lint --only lint.redundant-propagation --fix $file
  assert fixed.status.exited_with(0), fixed.stderr
  let expected = source.replace("(run.text false)?", with: "run.text false")
  assert file.read_text()? == expected
  let after = test.expect(ctx, expected, status: 3)?
  assert before.stdout == "cleanup\n" and after.stdout == before.stdout
  for report in [before.stderr, after.stderr] {
    assert "`false` exited 1" in report, report
  }
}

test test_nominal_prefer_fail_finding_allows_implicit_message_fix { |ctx|
  let source = r"""error LoadError = Failed(message: Str)

proc load() -> Result[Int] {
  return Err(LoadError.Failed(message: "no configuration"))
}

print load()?
"""
  let expected = source.replace("Failed(message: Str)", with: "Failed").replace(
    "Failed(message: \"no configuration\")",
    with: "Failed(\"no configuration\")",
  )
  let before = test.expect(ctx, source, status: 3)?
  let file = test.temp_file(ctx, name: "implicit-message.xsh", contents: bytes.from_text(source))?
  let fixed = run.capture --text "xsht" lint --only lint.prefer-fail,lint.prefer-implicit-message --fix $file
  assert file.read_text()? == expected, file.read_text()?
  assert fixed.status.exited_with(0), fixed.stderr
  let nominal = run.capture --text "xsht" lint --only lint.prefer-fail $file
  assert nominal.status.exited_with(1), nominal.stderr
  let remaining = run.capture --text "xsht" lint --only lint.prefer-implicit-message $file
  assert remaining.status.exited_with(0), remaining.stderr
  let after = test.expect(ctx, expected, status: 3)?
  assert after.stdout == before.stdout
  for report in [before.stderr, after.stderr] {
    assert "LoadError.Failed" in report and "no configuration" in report, report
  }
}
