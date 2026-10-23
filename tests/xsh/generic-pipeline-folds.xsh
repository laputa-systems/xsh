test generic_folds { |ctx|
  let source = p"tests/fixtures/runtime/generic-pipeline-folds.xsh".read_text()?
  test.expect(ctx, source + "\ngeneric_folds()?\n", status: 0)?
}
