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

# Functions that each hold one propagating form, at a known line.
const PROPAGATION_SITES = """error Stop = Bad(message: Str)

proc number(x: Int) [error] -> Result[Int] {
  return Err(Stop.Bad(message: "no number")) when x == 2
  Ok(x)
}

proc ready(x: Int) [error] -> Result[Bool] {
  return Err(Stop.Bad(message: "not ready")) when x == 2
  Ok(true)
}

proc release(x: Int) [error] -> Result[Unit] {
  return Err(Stop.Bad(message: "not released")) when x == 2
  Ok()
}

proc bound(x: Int) [error] -> Result[Int] {
  let value = number(x)?
  Ok(value)
}

proc tail(x: Int) [error] -> Result[Int] {
  number(x)?
}

proc condition(x: Int) [error] -> Result[Int] {
  if ready(x) {
    return Ok(1)
  }
  Ok(0)
}

proc converted(text: Str) [error] -> Result[Int] {
  let value = text as Int
  Ok(value)
}

proc deferred(x: Int) [error] -> Result[Int] {
  defer release(x)
  Ok(x)
}

proc failed(x: Int) [error] -> Result[Int] {
  fail "no value" when x == 2
  Ok(x)
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

# The replacement need not differ from the error it replaces. An operand that
# handles an `Err` has dealt with it, whatever it produces next: a new error
# with the same kind and message is still reported from where it propagates.
test test_an_error_replaced_by_an_equal_one_gets_its_own_traceback { |ctx|
  let replacements = [
    """  let value = (nested_outer(2) ?? Err(Stop.Bad(message: "handled failure")))?
  value""",
    """  let value = (match handled_outer(2) {
    Ok(v) => Ok(v)
    Err(_) => Err(Stop.Bad(message: "handled failure"))
  })?
  value""",
    """  unit_outer(2) ?? Err(Stop.Bad(message: "handled failure"))
  1""",
  ]
  for replacement in replacements {
    let failed = test.expect(
      ctx,
      HANDLED_PRELUDE + REPLACED_PRELUDE + "proc replacer() [error] -> Result[Int] {\n" + replacement + "\n}\n\nproc caller() [error] -> Result[Int] {\n  replacer()?\n}\n\ncaller()?\n",
      status: 3,
      stderr: ["handled failure", "proc caller", "proc replacer"],
    )?
    assert "proc handled_outer" not in failed.stderr, failed.stderr
    assert "proc nested_outer" not in failed.stderr, failed.stderr
    assert "proc unit_outer" not in failed.stderr, failed.stderr
  }
}

# A traceback starts at the form that propagates the failure. Each of these
# sits inside a called function: the place reported is the form's own, in that
# function, and the call that reached the function is the last frame.
test test_a_propagation_inside_a_function_reports_its_own_place { |ctx|
  let sites = [
    {call: "bound(2)", at: ":19:15-19:25", frame: "proc bound"},
    {call: "tail(2)", at: ":24:3-24:13", frame: "proc tail"},
    {call: "condition(2)", at: ":28:6-28:14", frame: "proc condition"},
    {call: "converted(\"wide\")", at: ":35:15-35:26", frame: "proc converted"},
    {call: "deferred(2)", at: ":40:9-40:19", frame: "proc deferred"},
  ]
  for site in sites {
    let _ = test.expect(
      ctx,
      PROPAGATION_SITES + "proc caller() [error] -> Result[Int] {\n  let value = " + site.call + "?\n  Ok(value)\n}\n\ncaller()?\n",
      status: 3,
      stderr: [site.at + "\n", "proc caller", site.frame],
    )?
  }
}

# `fail` returns its error as `return Err(...)` does: nothing has propagated
# yet, so the traceback starts at the caller's `?` and has no frame for the
# function that returned.
test test_a_returned_error_is_reported_from_the_propagation_that_meets_it { |ctx|
  let failed = test.expect(
    ctx,
    PROPAGATION_SITES + "proc caller() [error] -> Result[Int] {\n  let value = failed(2)?\n  Ok(value)\n}\n\ncaller()?\n",
    status: 3,
    stderr: ["no value", ":50:15-50:25\n", "proc caller"],
  )?
  assert "proc failed" not in failed.stderr, failed.stderr
}
