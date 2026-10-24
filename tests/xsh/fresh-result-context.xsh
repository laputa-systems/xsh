test fresh_result_context { |ctx|
  let source = p"tests/fixtures/runtime/fresh-result-context.xsh".read_text()?
  test.expect(ctx, source + "\nfresh_result_context()?\n", status: 0)?
}
