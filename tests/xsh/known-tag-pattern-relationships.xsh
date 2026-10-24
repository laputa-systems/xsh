test test_known_tag_patterns_select_the_declared_family { |ctx|
  let source = p"tests/fixtures/runtime/known-tag-pattern-relationships.xsh".read_text()?
  let output = test.run_xsh(ctx, source)?
  assert output.success, output.stderr
  assert output.stdout == ""
}
