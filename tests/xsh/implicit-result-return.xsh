type StringListResult = Result[List[Str]]

error ReturnError = Failure(message: Str) : InvalidData

proc build() [error] -> Result[List[Str]] {
  let built = ["ok"]
  built
}

proc fail_explicitly(fail = true) [error] -> Result[List[Str]] {
  return Err(ReturnError.Failure("explicit failure")) when fail
  ["ok"]
}

proc fail_leaf() [error] -> Result[Int] {
  Err(ReturnError.Failure("propagated failure"))
}

proc fail_through_question() [error] -> Result[Int] {
  fail_leaf()?
}

proc implicit_unit() [error] {
  assert 1 == 1
}

proc build_bare_through_alias() [error] -> StringListResult {
  let built = ["ok"]
  built
}

proc leaf(value: Int) [error] -> Result[Int] {
  value
}

proc middle(value: Int) [error] -> Result[Int] {
  leaf(value)?
}

test test_value_returning_error_helper { |ctx|
  let output = test.run_script(
    ctx,
    """
proc parse_uint(s: Str, min: Int) [error] -> Int {
  let value = s.parse_int()?
  if value < min {
    return min
  }
  return value
}

print parse_uint("42", 10)
""",
    [],
    {},
    b"",
    "parse-uint.xsh",
  )?

  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """42
"""

  let invalid = test.run_script(
    ctx,
    """
proc parse_uint(s: Str, min: Int) [error] -> Int {
  let value = s.parse_int()?
  if value < min {
    return min
  }
  return value
}

print parse_uint("not-a-number", 10)
""",
    [],
    {},
    b"",
    "parse-uint-invalid.xsh",
  )?

  let rejected = invalid.status != 0
  let rejection_details = invalid.stderr
  assert rejected, rejection_details
  assert "parse-int: invalid integer" in invalid.stderr
}

test test_implicit_result_return_in_par_map {
  let values = [1, 2]
    |> par-map(jobs: 2) { |_|
      build()
    }

  assert values == [Ok(["ok"]), Ok(["ok"])]
  assert values[0]? == ["ok"]

  let block_values = [1, 2]
    |> par-map(jobs: 2) { |_|
      let built = ["ok"]
      built
    }

  assert block_values == [["ok"], ["ok"]]
}

test test_result_return_shapes_agree {
  assert build()? == ["ok"]
  implicit_unit()?

  if let Err(error) = fail_explicitly() {
    assert error.message == "explicit failure"
  } else {
    assert false
  }

  if let Err(error) = fail_through_question() {
    assert error.message == "propagated failure"
  } else {
    assert false
  }
}

test test_nested_result_calls_in_par_map {
  let values = [1, 2]
    |> par-map(jobs: 2) { |value|
      middle(value)
    }

  assert values == [Ok(1), Ok(2)]
  assert values[1]? == 2
}

test test_result_alias_return_shape {
  assert build_bare_through_alias()? == ["ok"]
}

test test_explicit_result_return_shapes { |ctx|
  let output = test.run_script(
    ctx,
    """type StringListResult = Result[List[Str]]

proc build_explicitly() [error] -> Result[List[Str]] {
  return Ok(["ok"])
}

proc build_through_alias() [error] -> StringListResult {
  return Ok(["ok"])
}

proc leaf(value: Int) [error] -> Result[Int] {
  return Ok(value)
}

proc middle(value: Int) [error] -> Result[Int] {
  leaf(value)?
}

let direct = build_explicitly()?
let alias = build_through_alias()?
print direct[0] alias[0] middle(3)?
""",
  )?

  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """ok ok 3
"""
}

test test_implicit_result_return_through_module { |ctx|
  let root = test.temp_dir(ctx, name: "implicit-result-module")?
  let module_dir = fp"${root}/lib"
  module_dir.mkdir()?
  fp"${module_dir}/helper.xsh".write("""
##! Helper module for implicit Result return coverage.

## Builds the fixed value through an implicit Result tail.
export proc build() [error] -> Result[List[Str]] {
  let built = ["ok"]
  built
}
""")?

  let output = test.run_script(
    ctx,
    """
use helper

let values = [1, 2] |> par-map(jobs: 2) { |_|
  helper.build()
}
print values[0]?[0] values[1]?[0]
""",
    [],
    {XSH_MODULE_PATH: module_dir.display()},
  )?

  let succeeded = output.status == 0
  let failure_details = output.stderr
  assert succeeded, failure_details
  assert output.stdout == """ok ok
"""
}
