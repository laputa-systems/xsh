test test_mutable_list_splices_preserve_opaque_item_fields { |ctx|
  let source = p"tests/fixtures/runtime/list-splice-opaque-fields.xsh".read_text()?
  let output = test.expect(ctx, source, status: 0)?
  assert output.stdout == "2\n"
}
