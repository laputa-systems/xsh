test retry_no_completion { |ctx|
  let source = p"tests/fixtures/runtime/retry-no-completion.xsh".read_text()?
  test.expect(ctx, source + "\nretry_no_completion()?\n", status: 0)?
}
