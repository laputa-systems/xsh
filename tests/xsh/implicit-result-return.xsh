type StringListResult = Result[List[Str]]

error ReturnError = Failure : InvalidData

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

proc block_tail_result(value: Int) [error] -> Result[Int] {
  {
    let doubled = value * 2
    leaf(doubled)
  }
}

proc block_tail_value(value: Int) [error] -> Result[Int] {
  {
    let doubled = value * 2
    doubled
  }
}

proc nested_block_tail_failure() [error] -> Result[Int] {
  {
    {
      fail_leaf()
    }
  }
}

proc branch_block_tail(value: Int) [error] -> Result[Int] {
  if value > 0 {
    {
      leaf(value)
    }
  } else {
    0
  }
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
  implicit_unit()

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

# A command tail completes a return type that accepts Unit: the command runs
# for effect and the body finishes with its Unit value.
test test_command_tail_completes_unit_accepting_return_types { |ctx|
  let output = test.expect(
    ctx,
    """proc show() -> Result[Any] {
  print "shown"
}

proc label() -> Any {
  print "labeled"
}

let _ = show()?
let _: Any = label()
""",
    status: 0,
  )?
  assert output.stdout == """shown
labeled
"""
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
  let module_dir = fp"{root}/lib"
  module_dir.mkdir()
  fp"{module_dir}/helper.xsh".write("""
##! Helper module for implicit Result return coverage.

## Builds the fixed value through an implicit Result tail.
export proc build() [error] -> Result[List[Str], Error] {
  let built = ["ok"]
  built
}
""")

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
    {XSH_MODULE_PATH: module_dir},
  )?

  let succeeded = output.status == 0
  let failure_details = output.stderr
  assert succeeded, failure_details
  assert output.stdout == """ok ok
"""
}

# A bare block that is the tail of a function is that function's tail: where
# the function returns `Result[T]`, the block's own tail may be a `T` or a
# `Result[T]`, and a `Result` is the function's result, not a value inside it.
test test_bare_block_tail_takes_the_function_tail_rule {
  assert block_tail_result(2)? == 4
  assert block_tail_value(2)? == 4
  assert branch_block_tail(3)? == 3
  assert branch_block_tail(0)? == 0
  if let Err(error) = nested_block_tail_failure() {
    assert error.message == "propagated failure"
  } else {
    assert false
  }
}

# The rule is the function's and no wider: a block tail of the wrong type, a
# `Result` of a `Result`, and a `Result` where the function returns a plain
# value are still rejected.
test test_bare_block_tail_keeps_the_function_tail_limits { |ctx|
  let helper = "proc leaf(value: Int) [error] -> Result[Int] {\n  value\n}\n"
  for body in [
    "proc wrong(value: Int) [error] -> Result[Int] {\n  {\n    f\"{value}\"\n  }\n}\n",
    "proc nested(value: Int) [error] -> Result[Int] {\n  {\n    Ok(leaf(value))\n  }\n}\n",
    "proc plain(value: Int) [error] -> Int {\n  {\n    leaf(value)\n  }\n}\n",
  ] {
    let output = test.run_script(ctx, helper + body)?
    assert ! output.success, body
    assert "check.type-mismatch" in output.stderr, f"{body}: {output.stderr}"
  }
}
