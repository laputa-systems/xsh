test generic_reduce_by { |ctx|
  let source = p"tests/fixtures/runtime/generic-pipeline-reduce-by.xsh".read_text()?
  test.expect(ctx, source + "\ngeneric_reduce_by()?\n", status: 0)?
  test.expect(ctx, source + "\ninvalid_reduce_key()?\n", status: 3)?
  test.expect(ctx, source + "\ninvalid_reduce_mode()?\n", status: 3)?
}
