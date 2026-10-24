test independent_guarded_handlers_share_a_callable_contract { |ctx|
  let modules = test.temp_dir(ctx, name: "guarded-correspondence")?
  for name in ["left_handler", "right_handler"] {
    fp"{modules}/{name}.xsh".write(fp"tests/fixtures/modules/guarded-correspondence/{name}.xsh".read_text()?)?
  }
  let source = p"tests/fixtures/runtime/guarded-correspondence.xsh".read_text()?
  test.expect(ctx, source + "\nverify_guarded_correspondence()?\n", status: 0, env: {XSH_MODULE_PATH: modules})?
}
