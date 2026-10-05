error TestError = DivisionByZero : InvalidData

proc safe_div(x: Int) [error] -> Result[Int] {
  return Err(TestError.DivisionByZero("division by zero")) when x == 0

  Ok(100 / x)
}

test test_par_map_explicit_propagation_stops_on_error { |ctx|
  let failed = test.run_script(
    ctx,
    """
error TestError = DivisionByZero(message: Str) : InvalidData

proc safe_div(x: Int) [error] -> Result[Int] {
  if x == 0 {
    return Err(TestError.DivisionByZero("division by zero"))
  }

  Ok(100 / x)
}

proc main() [error] -> Result[Unit] {
  let values = [10, 20, 0, 40]
  let results = values |> par-map { |x| safe_div(x)? }
  print $results.len()
}

main()?
""",
  )?

  {
    let assertion_condition = ! failed.success
    let assertion_message = failed.stderr
    assert assertion_condition, assertion_message
  }
  assert failed.status == 3
  assert failed.stdout == ""
  {
    let assertion_condition = "DivisionByZero" in failed.stderr or "division by zero" in failed.stderr
    let assertion_message = failed.stderr
    assert assertion_condition, assertion_message
  }
}

test test_par_map_all_ok {
  let results = [1, 2, 3]
    |> par-map { |x|
      safe_div(x)
    }

  assert results.len() == 3
  assert results[0] == Ok(100)
  assert results[1] == Ok(50)
  assert results[2] == Ok(33)
}

test test_par_map_collect_all_retains_nominal_error_data_in_order {
  let results = [10, 0, 20]
    |> par-map { |x|
      safe_div(x)
    }
  assert results.len() == 3
  assert results[0]? == 10
  assert results[2]? == 5
  if let Err(failure) = results[1] {
    assert failure is TestError.DivisionByZero
    assert failure.message == "division by zero"
  } else {
    assert false, "expected the middle item's nominal error"
  }
}

proc divide_all(xs: List[Int], parallel: Bool) [error] -> Result[List[Int]] {
  if parallel {
    xs |> par-map(jobs: 4) { |x| safe_div(x)? }
  } else {
    xs |> map { |x| safe_div(x)? }
  }
}

proc divide_total(xs: List[Int]) [error] -> Result[Map[Str, Int]] {
  xs
    |> par-map(jobs: 4) { |x| safe_div(x)? }
    |> reduce-by(sum: true) { |value| {key: "all", value} }
}

test test_par_map_propagation_fails_the_enclosing_function_like_map {
  let items = [10, 20, 0, 40, 50, 0, 25, 5]
  for parallel in [false, true] {
    if let Err(failure) = divide_all(items, parallel) {
      assert failure is TestError.DivisionByZero
    } else {
      assert false, "expected the stage's first failure as the function's Err"
    }
  }

  assert divide_all([10, 20, 25], true)? == [10, 5, 4]
  if let Err(failure) = divide_total(items) {
    assert failure is TestError.DivisionByZero
  } else {
    assert false, "expected the fused stage's first failure as the function's Err"
  }

  assert divide_total([10, 20])?["all"] == 15
}

test test_par_map_uncaught_propagation_traces_the_callers { |ctx|
  let failed = test.run_script(
    ctx,
    """
error Stop = Bad(message: Str)

proc check(x: Int) [error] -> Result[Int] {
  return Err(Stop.Bad(message: "bad item")) when x == 2
  Ok(x)
}

proc check_all(xs: List[Int]) [error] -> Result[List[Int]] {
  xs |> par-map(jobs: 4) { |x| check(x)? }
}

let checked = check_all([1, 2, 3, 4])?
print $checked.len()
""",
  )?
  assert ! failed.success, failed.stderr
  assert failed.status == 3, failed.stderr
  assert "err: " in failed.stderr, failed.stderr
  assert "proc check_all at" in failed.stderr, failed.stderr
  assert "stream stage" not in failed.stderr, failed.stderr
}
