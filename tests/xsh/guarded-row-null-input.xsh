test test_guarded_row_null_inputs_preserve_scope_execution { |ctx|
  let source = p"tests/fixtures/runtime/guarded-row-null-input.xsh".read_text()?
  let output = test.run_xsh(ctx, source + "\nverify_guarded_row_null_input()?\n")?
  assert output.success, output.stderr
  assert output.stdout == "1\n2\n3\n4\n", output.stdout
  assert output.stderr == "", output.stderr
}
