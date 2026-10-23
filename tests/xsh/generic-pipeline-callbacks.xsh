test generic_callbacks { |ctx|
  test.expect(ctx, p"tests/fixtures/runtime/generic-pipeline-callbacks.xsh".read_text()? + "\ngeneric_callbacks()?\n", status: 0)?
}
