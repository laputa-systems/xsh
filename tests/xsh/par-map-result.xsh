error TestError = DivisionByZero(message: Str) : InvalidData

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
  failed.status == 3
  failed.stdout == ""
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

  results.len() == 3
  results[0] == Ok(100)
  results[1] == Ok(50)
  results[2] == Ok(33)
}

test test_par_map_collect_all_retains_nominal_error_data_in_order {
  let results = [10, 0, 20] |> par-map { |x| safe_div(x) }
  (results.len()) == (3)
  (results[0]?) == (10)
  (results[2]?) == (5)
  if let Err(failure) = results[1] {
      (failure is TestError.DivisionByZero)
      (failure.message) == ("division by zero")
  } else {
    assert false, "expected the middle item's nominal error"
  }
}
