proc test_indexed_execution_fixture_runs_on_standard_path(ctx: TestContext) [fs, error] {
  let source = p"tests/fixtures/frontend-indexed/indexed-execution.xsh".read_text()?
  let output = test.run_script(ctx, source, [], {}, b"", "indexed-execution.xsh")?
  test.ok(output.success, output.stderr)?
  test.eq(
    output.stdout,
    """slice 13 120 true true
""",
  )?
}

proc test_indexed_method_call_fixture_runs_on_standard_path(ctx: TestContext) [fs, error] {
  let source = p"tests/fixtures/frontend-indexed/indexed-method-call.xsh".read_text()?
  let output = test.run_script(ctx, source, [], {}, b"", "indexed-method-call.xsh")?
  test.ok(output.success, output.stderr)?
  test.eq(
    output.stdout,
    """non-empty
""",
  )?
  test.eq(output.stderr, "")?
}

proc test_nested_bindings_shadow_and_restore_outer_scope(ctx: TestContext) [fs, error] {
  # Keep the source under fixtures: it deliberately violates the corpus shadowing lint.
  let source = p"tests/fixtures/runtime/lexical-shadowing.xsh".read_text()?
  let output = test.run_script(ctx, source, [], {}, b"", "lexical-shadowing.xsh")?
  test.ok(output.success, output.stderr)?
  test.eq(
    output.stdout,
    """ab;cd;
one;
a;
inner
outer
200
ab|outer
|outer
""",
  )?
  test.eq(output.stderr, "")?
}
