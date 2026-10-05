test test_indexed_execution_fixture_runs_on_standard_path { |ctx|
  let source = p"tests/fixtures/frontend-indexed/indexed-execution.xsh".read_text()?
  let output = test.expect(ctx, source, status: 0, args: [], env: {}, stdin: b"", name: "indexed-execution.xsh")?
  assert output.stdout == """slice 13 120 true true
"""
}

test test_indexed_method_call_fixture_runs_on_standard_path { |ctx|
  let source = p"tests/fixtures/frontend-indexed/indexed-method-call.xsh".read_text()?
  let output = test.expect(ctx, source, status: 0, args: [], env: {}, stdin: b"", name: "indexed-method-call.xsh")?
  assert output.stdout == """non-empty
"""
  assert output.stderr == ""
}

test test_nested_bindings_shadow_and_restore_outer_scope { |ctx|
  # Keep the source under fixtures: it deliberately violates the corpus shadowing lint.
  let source = p"tests/fixtures/runtime/lexical-shadowing.xsh".read_text()?
  let output = test.expect(ctx, source, status: 0, args: [], env: {}, stdin: b"", name: "lexical-shadowing.xsh")?
  assert output.stdout == """ab;cd;
one;
a;
inner
outer
200
ab|outer
|outer
"""
  assert output.stderr == ""
}
