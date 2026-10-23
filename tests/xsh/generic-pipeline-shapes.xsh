test generic_shapes { |ctx|
  let source = p"tests/fixtures/runtime/generic-pipeline-shapes.xsh".read_text()?
  test.expect(ctx, source + "\ngeneric_shapes()?\n", status: 0)?
}
