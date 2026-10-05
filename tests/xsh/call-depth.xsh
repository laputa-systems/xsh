# At most 50000 function calls are open at once: recursion that never ends is
# the runtime error `stack-overflow`, which names the innermost open calls.

test test_recursion_that_never_ends_is_a_stack_overflow_error { |ctx|
  let output = test.expect(
    ctx,
    """proc even(n: Int) -> Bool {
  !odd(n + 1)
}

proc odd(n: Int) -> Bool {
  !even(n + 1)
}

print f"{even(0)}"
""",
    status: 3,
    stderr: ["stack-overflow", "more than 50000 calls are open"],
  )?
  assert output.stdout == ""
  assert "the innermost are odd -> even -> odd -> even -> odd -> even" in output.stderr, output.stderr
  assert "overflowed its stack" not in output.stderr, output.stderr
}

test test_a_pure_function_that_never_returns_is_the_same_error { |ctx|
  test.expect(
    ctx,
    """pure count(n: Int) -> Int {
  count(n + 1) + 1
}

print f"{count(0)}"
""",
    status: 3,
    stderr: ["stack-overflow", "count -> count"],
  )?
}

test test_recursion_below_the_limit_returns { |ctx|
  let output = test.expect(
    ctx,
    """proc depth(n: Int) -> Int {
  if n == 0 {
    return 0
  }
  depth(n - 1) + 1
}

print f"{depth(9000)}"
""",
    status: 0,
  )?
  assert output.stdout == "9000\n"
}
