# A `?` failure records the traceback of the error it propagates. Once the
# caller handles that `Err`, a later failure must report its own error and
# call path, never the handled one's.

const HANDLED_PRELUDE = """error Stop = Bad(message: Str)

proc check(x: Int) [error] -> Result[Int] {
  return Err(Stop.Bad(message: "handled failure")) when x == 2
  Ok(x)
}

proc handled_outer(x: Int) [error] -> Result[Int] {
  check(x)?
}

proc fresh_failure() [error] -> Result[Int] {
  Err(Stop.Bad(message: "fresh failure"))
}

proc first_of(items: List[Int]) -> Int {
  items[0]
}

let empty: List[Int] = []
"""

const REPLACED_PRELUDE = """
proc nested_outer(x: Int) [error] -> Result[Result[Int]] {
  let value = check(x)?
  Ok(Ok(value))
}

proc unit_outer(x: Int) [error] -> Result[Result[Unit]] {
  let _ = check(x)?
  Ok(Ok())
}
"""

pure handled_script(handling: Str, failure: Str) -> Str {
  HANDLED_PRELUDE + handling + "\n" + failure + "\n"
}

test test_a_handled_error_leaves_no_traceback_for_a_later_runtime_error { |ctx|
  let handlers = [
    """match handled_outer(2) {
  Ok(v) => print $v
  Err(e) => print "handled"
}""",
    """if let Err(e) = handled_outer(2) {
  print "handled"
}""",
    """let fallback = handled_outer(2) ?? 0
print $fallback""",
    """let kept = handled_outer(2)
assert kept is Err(_)""",
  ]
  for handling in handlers {
    for failure in ["let first = empty[0]", "let first = first_of(empty)"] {
      let failed = test.expect(ctx, handled_script(handling, failure), status: 3, stderr: ["list index", "err: "])?
      assert "handled failure" not in failed.stderr, failed.stderr
      assert "handled_outer" not in failed.stderr, failed.stderr
    }
  }
}

test test_a_handled_error_in_the_same_statement_leaves_no_traceback { |ctx|
  let failed = test.expect(
    ctx,
    handled_script(
  "",
  """let first = match handled_outer(2) {
  Ok(v) => v
  Err(_) => empty[0]
}""",
),
    status: 3,
    stderr: ["list index"],
  )?
  assert "handled failure" not in failed.stderr, failed.stderr
  assert "handled_outer" not in failed.stderr, failed.stderr

  let propagated = test.expect(
    ctx,
    handled_script("", "let total = (handled_outer(2) ?? 0) + fresh_failure()?"),
    status: 3,
    stderr: ["fresh failure"],
  )?
  assert "handled failure" not in propagated.stderr, propagated.stderr
  assert "handled_outer" not in propagated.stderr, propagated.stderr
}

test test_a_handled_error_leaves_no_traceback_for_a_later_propagation { |ctx|
  let failed = test.expect(
    ctx,
    handled_script(
  "let fallback = handled_outer(2) ?? 0",
  """proc caller() [error] -> Result[Int] {
  let _ = match handled_outer(2) {
    Ok(v) => v
    Err(_) => 0
  }
  fresh_failure()?
}
caller()?""",
),
    status: 3,
    stderr: ["err: ", "fresh failure", "proc caller"],
  )?
  assert "handled failure" not in failed.stderr, failed.stderr
  assert "handled_outer" not in failed.stderr, failed.stderr
}

test test_a_par_map_worker_reports_its_own_failure_after_handling_one { |ctx|
  let failed = test.expect(
    ctx,
    handled_script(
  "",
  """proc check_all(xs: List[Int]) [error] -> Result[List[Int]] {
  xs |> par-map(jobs: 2) { |x|
    let fallback = handled_outer(x) ?? 0
    fresh_failure()?
  }
}
let checked = check_all([2])?
print $checked.len()""",
),
    status: 3,
    stderr: ["fresh failure", "proc check_all"],
  )?
  assert "handled failure" not in failed.stderr, failed.stderr
  assert "handled_outer" not in failed.stderr, failed.stderr
}

test test_a_repropagated_error_keeps_its_original_traceback { |ctx|
  let _ = test.expect(
    ctx,
    handled_script(
  "",
  """proc caller() [error] -> Result[Int] {
  let kept = handled_outer(2)
  print "kept"
  handled_outer(2)?
}
caller()?""",
),
    status: 3,
    stderr: ["handled failure", "proc handled_outer", "proc caller"],
  )?
}

# An `Err` that is handled and replaced inside one `?` operand, or inside one
# propagating statement, starts no statement and no call in between. The
# replacement is still a different error: its traceback starts at the `?` that
# propagates it, without the handled error's callee.
test test_an_error_replaced_inside_one_operand_gets_its_own_traceback { |ctx|
  let replacements = [
    """  let value = (nested_outer(2) ?? Err(Stop.Bad(message: "replacement")))?
  value""",
    """  let value = (match handled_outer(2) {
    Ok(v) => Ok(v)
    Err(_) => Err(Stop.Bad(message: "replacement"))
  })?
  value""",
    """  unit_outer(2) ?? Err(Stop.Bad(message: "replacement"))
  1""",
    """  assert (handled_outer(2) ?? 0) == 1, "replacement"
  1""",
  ]
  for replacement in replacements {
    let failed = test.expect(
      ctx,
      HANDLED_PRELUDE + REPLACED_PRELUDE + "proc replacer() [error] -> Result[Int] {\n" + replacement + "\n}\n\nproc caller() [error] -> Result[Int] {\n  replacer()?\n}\n\ncaller()?\n",
      status: 3,
      stderr: ["replacement", "proc caller", "proc replacer"],
    )?
    assert "handled failure" not in failed.stderr, failed.stderr
    assert "proc handled_outer" not in failed.stderr, failed.stderr
    assert "proc nested_outer" not in failed.stderr, failed.stderr
    assert "proc unit_outer" not in failed.stderr, failed.stderr
  }
}
