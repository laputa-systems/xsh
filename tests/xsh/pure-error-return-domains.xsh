error PureReturnProblem = Stopped

pure pure_return_problem() -> Error {
  PureReturnProblem.Stopped()
}

pure pure_return_attempt(flag, value) {
  if flag { return Err(pure_return_problem()) }
  Ok(value)
}

pure pure_return_string(flag: Bool, value: Int) -> Result[Int, Str] {
  if flag { return Err("stop") }
  Ok(value)
}

test pure_error_returns_preserve_explicit_payloads_and_nominal_errors {
  let integer: Result[Int] = pure_return_attempt(false, 7)
  let word: Result[Str] = pure_return_attempt(false, "kept")
  assert integer == Ok(7)
  assert word == Ok("kept")
  assert match pure_return_attempt(true, 7) { Err(_) => true; Ok(_) => false }
  assert match pure_return_string(true, 7) { Err(problem) => problem == "stop"; Ok(_) => false }
}

test pure_error_returns_reject_incompatible_payloads_during_check { |ctx|
  for source in [
    r"""pure attempt(flag, value) {
  if flag { return Err("stop") }
  Ok(value)
}
let result = attempt(true, 7)
""",
    r"""pure attempt(flag, value) {
  if flag { return Err("stop") }
  if !flag { return Err(error.failure("structured")) }
  Ok(value)
}
let result = attempt(true, 7)
""",
    r"""pure attempt(flag: Bool, value: Int) {
  if flag { return Err("stop") }
  Ok(value)
}
let result = attempt(true, 7)
match result { Err(problem) => print $problem.message; Ok(_) => print "unexpected" }
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.status == 2, output.stderr
    assert "[check.type-mismatch]" in output.stderr, output.stderr
    assert "compact.indexed-build" not in output.stderr, output.stderr
  }
}
