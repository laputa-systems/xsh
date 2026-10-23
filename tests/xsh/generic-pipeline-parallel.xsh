test generic_parallel { |ctx|
  let source = p"tests/fixtures/runtime/generic-pipeline-parallel.xsh".read_text()?
  test.expect(ctx, source + "\ngeneric_parallel()?\n", status: 0)?
}
