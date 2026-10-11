test recursion_through_a_stage_block_reports_stack_overflow { |ctx|
  let output = test.expect(
    ctx,
    r"""proc descend(n: Int) -> Int {
  let values = [n] |> map { |item|
    let next = item + 1
    descend(next)
  }
  values[0]
}

print ${descend(0)}
""",
    status: 3,
    stderr: ["stack-overflow"],
    env: {XSH_TEST_SMALL_EVAL_STACK: "1"},
  )?
  assert "overflowed its stack" not in output.stderr, output.stderr
}

test recursion_through_a_loop_expression_reports_stack_overflow { |ctx|
  let output = test.expect(
    ctx,
    r"""proc descend(n: Int) -> Int {
  let result = loop { break descend(n + 1) }
  result
}

print ${descend(0)}
""",
    status: 3,
    stderr: ["stack-overflow"],
    env: {XSH_TEST_SMALL_EVAL_STACK: "1"},
  )?
  assert "overflowed its stack" not in output.stderr, output.stderr
}

test recursion_through_a_deferred_call_reports_stack_overflow { |ctx|
  let output = test.expect(
    ctx,
    r"""proc descend(n: Int) -> Result[Unit] {
  defer descend(n + 1)
  Ok()
}

descend(0)?
""",
    status: 3,
    stderr: ["stack-overflow"],
    env: {XSH_TEST_SMALL_EVAL_STACK: "1"},
  )?
  assert "overflowed its stack" not in output.stderr, output.stderr
}

test caught_failure_restores_the_machine_depth { |ctx|
  let output = test.expect(
    ctx,
    r"""error DescentError = Bottom(message: Str)

proc descend(n: Int) -> Result[Int, DescentError] {
  if n == 0 { return Err(DescentError.Bottom(message: "bottom")) }
  let values = [n] |> map { |item|
    let next = item - 1
    descend(next)?
  }
  Ok(values[0])
}

proc increment(n: Int) -> Int { n + 1 }

match try { descend(12)? } {
  Err(error) => assert error.message == "bottom"
  Ok(_) => assert false, "recursive stage unexpectedly succeeded"
}

for n in range(40) {
  let values = [n] |> map { |item|
    let next = increment(item)
    next
  }
  assert values == [n + 1]
}
print restored
""",
    status: 0,
    stdout: ["restored\n"],
    env: {XSH_TEST_SMALL_EVAL_STACK: "1"},
  )?
  assert output.stderr == "", output.stderr
}
