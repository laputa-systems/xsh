test test_bool_tails_of_unit_bodies_are_rejected { |ctx|
  for source in [
    """proc statement_tail() [fs, error] -> Unit { p".".exists()? }
statement_tail()
""",
    """proc result_tail() [fs, error] -> Result[Unit] { p".".exists()? }
result_tail()?
""",
    """pure false_value() -> Bool { false }
false_value() == false
""",
    """pure result_value() -> Result[Bool] { Ok(false) }
match result_value() { Ok(value) => value == false; Err(_) => false }
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    assert "check.bool-statement" in output.stderr, output.stderr
    assert "assert <expr>" in output.stderr, output.stderr
  }
}

test test_explicit_assert_tails_and_bool_value_tails { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc statement_tail() [fs, error] -> Unit { assert p".".exists()? }
proc result_tail() [fs, error] -> Result[Unit] { assert p".".exists()? }
proc bool_tail() [fs, error] -> Bool { p".".exists()? }
pure false_value() -> Bool { false }
statement_tail()
result_tail()?
assert bool_tail()
assert false_value() == false
let _ = false_value()
print "done"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """done
"""
}

test test_result_bool_statement_tail_requires_explicit_propagation { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc statement_tail() [fs, error] -> Unit { p".".exists() }
statement_tail()
""",
  )?
  assert ! output.success
  assert "check.ignored-result" in output.stderr, output.stderr
}

test test_false_assert_tail_captures_after_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc cleanup() [io] -> Unit { print "cleaned" }
proc statement_tail() [fs, io, error] -> Unit {
  defer cleanup()
  assert p"xsh-assertion-tail-context-missing-entry".exists()?
}
let result = try { statement_tail() }
match result {
  Err(AssertionError.Failed {message: _}) => print "assertion"
  _ => print "unexpected"
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """cleaned
assertion
"""
}
