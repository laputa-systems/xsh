# At most 100000 function calls are open at once: recursion that never ends is
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
    stderr: ["stack-overflow", "more than 100000 calls are open"],
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

# A call costs the same however many calls are open. Twenty thousand open
# calls return in a fraction of a second; were each to read the calls beneath
# it, the same recursion would take many seconds and miss this limit.
test test_deep_recursion_returns_promptly { |ctx|
  test.timeout(ctx, 3s)
  let output = test.expect(
    ctx,
    """pure descend(n: Int) -> Int {
  if n <= 0 {
    return 0
  }

  return 1 + descend(n - 1)
}

proc climb(n: Int) -> Int {
  if n > 0 {
    return climb(n - 1) + 1
  }
  0
}

print f"{descend(20000)} {climb(20000)}"
""",
    status: 0,
  )?
  assert output.stdout == "20000 20000\n"
}

# An open `ctx` block beneath the recursion is found without reading the
# calls above it, and still describes a failure that passes through it.
test test_deep_recursion_inside_a_context_block_returns_promptly { |ctx|
  test.timeout(ctx, 3s)
  let output = test.expect(
    ctx,
    """proc climb(n: Int) [error] -> Result[Int] {
  if n > 0 {
    return Ok(climb(n - 1)? + 1)
  }
  fail "bottom reached" when n < 0
  Ok(0)
}

proc run_all() [error, io] {
  ctx "climbing" {
    print f"{climb(20000)?}"
    let _ = climb(-1)?
  }
}

run_all()
""",
    status: 3,
    stdout: ["20000\n"],
    stderr: ["bottom reached", "climbing"],
  )?
  let _ = output
}
