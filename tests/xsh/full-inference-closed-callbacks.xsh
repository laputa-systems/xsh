test test_closed_optional_callbacks_preserve_imported_contracts { |ctx|
  let root = test.temp_dir(ctx, name: "closed-callback-import")?
  fp"{root}/closed-callback-provider.xsh".write(p"tests/fixtures/runtime/closed-callback-provider.xsh".read_text()?)
  let source = p"tests/fixtures/runtime/closed-callback-import.xsh".read_text()? + "\nverify_closed_callbacks()?\n"
  let output = test.run_xsh(ctx, source, env: {XSH_MODULE_PATH: root})?
  assert output.success, output.stderr
  assert output.stdout == "observer:7\nobserver:4\nobserver:-1\n"
}
