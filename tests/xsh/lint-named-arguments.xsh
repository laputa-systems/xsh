test test_named_argument_pun_fix_preserves_resolution_comments_and_converges { |ctx|
  let rule = "lint.prefer-named-argument-pun"
  let source = p"tests/fixtures/syntax/valid/named-argument-pun-explicit.xsh".read_text()?
  let file = test.temp_file(ctx, name: "named-pun.xsh", contents: bytes.from_text(source))?
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr

  let reported = run.capture --text "xsht" lint --only $rule $file
  let findings = reported.stderr.split(f"warn[{rule}]")
  # Different identifiers and field expressions are not puns.
  assert findings.len() == 4, reported.stderr
  assert ":9:20\n" in findings[1] and "help: omit the repeated value name -> value:\n" in findings[1], findings[1]
  assert ":10:22\n" in findings[2] and "help: omit the repeated value name -> value:\n" in findings[2], findings[2]
  # Comments prevent safe replacement.
  assert ":13:24\n" in findings[3] and "help: " not in findings[3], findings[3]

  let _ = run.capture --text "xsht" lint --fix --only $rule $file
  let fixed = file.read_text()?
  assert fixed == source.replace("accept(value: value)", with: "accept(value:)"), fixed
  assert "# Preserve this comment." in fixed, fixed
  let rechecked = run.capture --text "xsht" check $file
  assert rechecked.status.exited_with(0), rechecked.stderr
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr

  # What is left has no fix: a second run changes nothing.
  let again = run.capture --text "xsht" lint --fix --only $rule $file
  assert "help: " not in again.stderr, again.stderr
  assert file.read_text()? == fixed
}
