test test_resource_scope_return_preserves_release_errors_and_primary_flow { |ctx|
  let source = p"tests/fixtures/runtime/resource-scope-return.xsh".read_text()?
  let output = test.run_xsh(ctx, source + "\nverify_resource_scope_return()?\n")?
  assert output.success, output.stderr
  assert output.stdout == "", output.stdout
  assert "cleanup error [fs-lock]" in output.stderr, output.stderr
  assert "lock handle is not active" in output.stderr, output.stderr
}
