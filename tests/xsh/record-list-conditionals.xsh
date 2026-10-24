test record_list_conditionals { |ctx|
  let source = p"tests/fixtures/runtime/record-list-conditionals.xsh".read_text()?
  test.expect(ctx, source + "\nrecord_list_conditionals()?\n", status: 0)?
}
