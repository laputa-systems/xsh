test test_propagated_bool_statement_tails_assert_without_unit_value_context { |ctx|
  let output = test.run_script(ctx, r"""
proc statement_tail() [fs, error] -> Unit { p".".exists()? }
proc result_tail() [fs, error] -> Result[Unit] { p".".exists()? }
proc bool_tail() [fs, error] -> Bool { p".".exists()? }
pure false_value() -> Bool { false }
pure result_value() -> Result[Bool] { Ok(false) }
statement_tail()
result_tail()?
bool_tail()
false_value() == false
match result_value() { Ok(value) => value == false; Err(_) => false }
print "done"
""")?
  assert output.success, output.stderr
  output.stdout == "done\n"
}

test test_result_bool_statement_tail_requires_explicit_propagation { |ctx|
  let output = test.run_script(ctx, r"""
proc statement_tail() [fs, error] -> Unit { p".".exists() }
statement_tail()
""")?
  !output.success
  assert "check.ignored-result" in output.stderr, output.stderr
}

test test_propagated_false_statement_tail_captures_after_cleanup { |ctx|
  let output = test.run_script(ctx, r"""
proc cleanup() [io] -> Unit { print "cleaned" }
proc statement_tail() [fs, io, error] -> Unit {
  defer cleanup()
  p"xsh-assertion-tail-context-missing-entry".exists()?
}
let result = try { statement_tail() }
match result {
  Err(AssertionError.Failed {message: _}) => print "assertion"
  _ => print "unexpected"
}
""")?
  assert output.success, output.stderr
  output.stdout == "cleaned\nassertion\n"
}
