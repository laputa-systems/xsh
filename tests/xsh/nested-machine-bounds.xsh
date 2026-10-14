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

# The top-level statement owns one machine; each producer pull owns one more.
# `for` keeps each parent pull open while it asks its child for an item, so
# nested(30) opens 31 producer machines and reaches 32 total. nested(31) tries
# to open the 33rd machine at the child pull.
test nested_producer_pull_rejects_33rd_frame_machine { |ctx|
  let accepted_marker = test.temp_path(ctx, name: "producer-pull-accepted")
  let rejected_marker = test.temp_path(ctx, name: "producer-pull-rejected")
  let source = r"""stream nested(n: Int, marker: Path) [fs, error] -> Stream[Int] {
  if n == 0 {
    marker.write("pulled")?
    yield 42
  } else {
    for value in nested(n - 1, marker) {
      yield value
    }
  }
}
let depth = args[0].parse_int()?
let marker = Path(args[1])
let source = nested(depth, marker)
print f"constructed {marker.exists()?}"
let values = source |> collect()
print f"accepted {values[0]}"
"""

  let accepted = test.expect(
    ctx,
    source,
    status: 0,
    stdout: ["constructed false\naccepted 42\n"],
    args: ["30", accepted_marker],
    env: {XSH_TEST_SMALL_EVAL_STACK: "1"},
  )?
  assert accepted.stderr == "", accepted.stderr
  assert accepted_marker.read_text()? == "pulled"

  let rejected = test.expect(
    ctx,
    source,
    status: 3,
    stdout: ["constructed false\n"],
    stderr: [
      "stack-overflow",
      "more than 32 frame machines are open",
      "for value in nested(n - 1, marker)",
    ],
    args: ["31", rejected_marker],
    env: {XSH_TEST_SMALL_EVAL_STACK: "1"},
  )?
  assert "overflowed its stack" not in rejected.stderr, rejected.stderr
  assert ! rejected_marker.exists()?
}

# The top-level statement contributes one machine; the flat-map callback runs
# on that statement's machine, and nested(30) starts 31 producer frames. Early
# termination cancels those same 31 started producers at the accepted 32-machine
# boundary. An additional started child would first require a 33rd nested pull,
# so this script path cannot independently reach a 33rd cancellation entry.
test nested_started_callback_producer_cancel_cleans_at_32_machine_boundary { |ctx|
  let root = test.temp_dir(ctx, name: "nested-producer-cancel")?
  let pid_file = fp"{root}/child.pid"
  defer root.remove()
  defer {
    if let Ok(text) = pid_file.read_text() {
      if let Ok(pid) = text.trim().parse_int() {
        if process.kill(pid, signal: "0") is Ok(_) {
          let _ = process.kill(pid, signal: "KILL")
        }
      }
    }
  }

  let source = r"""stream nested(n: Int, root: Path, pid_file: Path) [fs, process, error] -> Stream[Int] {
  tempdir scratch at fp"{root}/{n}" {
    if n == 0 {
      let child = spawn run sleep 30 ?
      defer child.cancel(kill_after: 0ms)
      pid_file.write(f"{child.pid}")?
      yield child.pid
    } else {
      for value in nested(n - 1, root, pid_file) {
        yield value
      }
    }
  }
}
let root = Path(args[0])
let pid_file = Path(args[1])
let source = range(1)
  |> flat-map { |_| nested(30, root, pid_file) }
  |> take(1)
let _ = source |> first()?
print "cancelled"
"""
  let output = test.expect(
    ctx,
    source,
    status: 0,
    stdout: ["cancelled\n"],
    args: [root, pid_file],
    env: {XSH_TEST_SMALL_EVAL_STACK: "1"},
  )?
  assert output.stderr == "", output.stderr

  for n in range(31) {
    assert ! fp"{root}/{n}".exists()?, f"producer {n} kept its tempdir after cancellation"
  }
  let child_pid = pid_file.read_text()?.trim() as Int
  assert process.kill(child_pid, signal: "0") is Err(_), "the leaf producer must reap its child during cancellation"
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
