test test_dynamic_errors_require_validation_before_propagation { |ctx|
  for source in [
    "pure value() -> Result[Int] { Ok(1) }\nlet dynamic: Pure = value\nmatch dynamic.call() { Ok(_) => {}; Err(error) => Err(error)? }",
    "proc forward(raw: Any) [error] -> Result[Int] { Err(raw)? }",
    "stream forward(raw: Any) [error] -> Stream[Int] { yield 1; Err(raw)? }",
  ] {
    let output = test.run_xsh(ctx, source)?
    assert output.status == 2
    assert "check.dynamic-boundary" in output.stderr, output.stderr
    assert !("compact.indexed-build" in output.stderr), output.stderr
  }
}

test test_validated_dynamic_error_retains_its_family { |ctx|
  let output = test.run_xsh(ctx, r"""error Failure = Raised(message: Str)
pure fail() -> Result[Int, Failure] { Err(Failure.Raised("expected")) }
let dynamic: Pure = fail
match dynamic.call() {
  Ok(_) => {}
  Err(error) => Err(error.require(Error)?)?
}
""")?
  assert output.status == 3, output.stderr
  assert "Failure.Raised" in output.stderr, output.stderr
}

test test_opaque_error_data_stays_opaque { |ctx|
  let output = test.run_xsh(ctx, r"""pure store(value: Any) -> Any { Err(value) }
let data: Any = store(7)
match data { Err(value) => print ${json.encode(value)?}; _ => print "unexpected" }
""")?
  assert output.status == 0, output.stderr
  assert output.stdout == "7\n", output.stdout
}
