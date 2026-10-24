test resource_scope_failure_value_tail { |ctx|
  let source = p"tests/fixtures/runtime/resource-scope-failure-tail.xsh".read_text()?
  let runtime = p"tests/fixtures/runtime/resource-scope-failure-tail-runtime.xsh".read_text()?
  test.expect(ctx, source + "\n" + runtime + "\nverify_resource_failure_tail()?\n", status: 0)?
}
