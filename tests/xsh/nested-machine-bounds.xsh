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

test finite_recursion_through_deferred_blocks_uses_the_active_machine { |ctx|
  let output = test.expect(
    ctx,
    r"""proc unwind(n: Int) -> Result[Unit] {
  defer {
    if n > 0 { unwind(n - 1)? }
  }
  Ok()
}

unwind(40)?
print "done"
""",
    status: 0,
    stdout: ["done\n"],
    env: {XSH_TEST_SMALL_EVAL_STACK: "1"},
  )?
  assert output.stderr == "", output.stderr
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

test nested_producer_collection_accepts_32_machines_and_rejects_33 { |ctx|
  # The top-level collection statement stays open while nested(30) opens 31 producers.
  let source = r"""stream nested(n: Int) -> Stream[Int] {
  if n == 0 {
    yield 42
  } else {
    let values = nested(n - 1) |> collect()
    yield @values
  }
}
let depth = args[0].parse_int()?
let values = nested(depth) |> collect()
print f"accepted {values[0]}"
"""
  let accepted = test.expect(
    ctx,
    source,
    status: 0,
    stdout: ["accepted 42\n"],
    args: ["30"],
    env: {XSH_TEST_SMALL_EVAL_STACK: "1"},
  )?
  assert accepted.stderr == "", accepted.stderr

  let rejected = test.expect(
    ctx,
    source,
    status: 3,
    stderr: ["stack-overflow", "more than 32 frame machines are open"],
    args: ["31"],
    env: {XSH_TEST_SMALL_EVAL_STACK: "1"},
  )?
  assert rejected.stdout == "", rejected.stdout
  assert "overflowed its stack" not in rejected.stderr, rejected.stderr
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
