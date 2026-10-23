test generic_keys { |ctx|
  let source = p"tests/fixtures/runtime/generic-pipeline-keys.xsh".read_text()?
  test.expect(ctx, source + "\ngeneric_keys()?\n", status: 0)?
}
