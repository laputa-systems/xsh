error TestError = DivisionByZero(message: Str) : InvalidData

proc safe_div(x: Int) [error] -> Result[Int] {
  if x == 0 {
    return Err(TestError.DivisionByZero("division by zero"))
  }

  Ok(100 / x)
}

test test_par_map_explicit_propagation_stops_on_error [error] { |ctx|
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

  test.ok(! failed.success, failed.stderr)?
  test.eq(failed.status, 3)?
  test.eq(failed.stdout, "")?
  test.ok("DivisionByZero" in failed.stderr or "division by zero" in failed.stderr, failed.stderr)?
}

test test_par_map_all_ok [error] {
  let results = [1, 2, 3]
    |> par-map { |x|
      safe_div(x)
    }

  test.eq(results.len(), 3)?
  test.eq(results[0], Ok(100))?
  test.eq(results[1], Ok(50))?
  test.eq(results[2], Ok(33))?
}

test test_par_map_collect_all_retains_nominal_error_data_in_order [error] {
  let results = [10, 0, 20] |> par-map { |x| safe_div(x) }
  test.eq(results.len(), 3)?
  test.eq(results[0]?, 10)?
  test.eq(results[2]?, 5)?
  match results[1] {
    Err(failure) => {
      test.ok(failure is TestError.DivisionByZero)?
      test.eq(failure.message, "division by zero")?
    }
    _ => test.ok(false, "expected the middle item's nominal error")?
  }
}
