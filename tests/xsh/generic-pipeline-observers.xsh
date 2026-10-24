test generic_observers { |ctx|
  let source = p"tests/fixtures/runtime/generic-pipeline-observers.xsh".read_text()?
  test.expect(ctx, source + "\ngeneric_observers()?\n", status: 0)?
}
