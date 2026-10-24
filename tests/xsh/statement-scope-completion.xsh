test test_statement_scope_completions_preserve_results_and_cleanup { |ctx|
  let source = p"tests/fixtures/runtime/statement-scope-completion.xsh".read_text()?
  let output = test.run_xsh(ctx, source + "\nverify_statement_scope_completion()?\n")?
  assert output.success, output.stderr
  assert output.stdout == "", output.stdout
  assert output.stderr == "", output.stderr
}
