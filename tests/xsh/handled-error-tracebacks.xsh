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
      let failed = test.run_script(ctx, handled_script(handling, failure))?
      assert failed.status == 3, failed.stderr
      assert "list index" in failed.stderr, failed.stderr
      assert "err: " in failed.stderr, failed.stderr
      assert "handled failure" not in failed.stderr, failed.stderr
      assert "handled_outer" not in failed.stderr, failed.stderr
    }
  }
}

test test_a_handled_error_in_the_same_statement_leaves_no_traceback { |ctx|
  let failed = test.run_script(
    ctx,
    handled_script(
  "",
  """let first = match handled_outer(2) {
  Ok(v) => v
  Err(_) => empty[0]
}""",
),
  )?
  assert failed.status == 3, failed.stderr
  assert "list index" in failed.stderr, failed.stderr
  assert "handled failure" not in failed.stderr, failed.stderr
  assert "handled_outer" not in failed.stderr, failed.stderr

  let propagated = test.run_script(
    ctx,
    handled_script("", "let total = (handled_outer(2) ?? 0) + fresh_failure()?"),
  )?
  assert propagated.status == 3, propagated.stderr
  assert "fresh failure" in propagated.stderr, propagated.stderr
  assert "handled failure" not in propagated.stderr, propagated.stderr
  assert "handled_outer" not in propagated.stderr, propagated.stderr
}

test test_a_handled_error_leaves_no_traceback_for_a_later_propagation { |ctx|
  let failed = test.run_script(
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
  )?
  assert failed.status == 3, failed.stderr
  assert "err: " in failed.stderr, failed.stderr
  assert "fresh failure" in failed.stderr, failed.stderr
  assert "proc caller" in failed.stderr, failed.stderr
  assert "handled failure" not in failed.stderr, failed.stderr
  assert "handled_outer" not in failed.stderr, failed.stderr
}

test test_a_par_map_worker_reports_its_own_failure_after_handling_one { |ctx|
  let failed = test.run_script(
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
  )?
  assert failed.status == 3, failed.stderr
  assert "fresh failure" in failed.stderr, failed.stderr
  assert "proc check_all" in failed.stderr, failed.stderr
  assert "handled failure" not in failed.stderr, failed.stderr
  assert "handled_outer" not in failed.stderr, failed.stderr
}

test test_a_repropagated_error_keeps_its_original_traceback { |ctx|
  let failed = test.run_script(
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
  )?
  assert failed.status == 3, failed.stderr
  assert "handled failure" in failed.stderr, failed.stderr
  assert "proc handled_outer" in failed.stderr, failed.stderr
  assert "proc caller" in failed.stderr, failed.stderr
}
