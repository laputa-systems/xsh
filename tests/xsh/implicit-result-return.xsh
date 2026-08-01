type StringListResult = Result[List[Str]]

error ReturnError = Failure(message: Str) : InvalidData

proc build() [error] -> Result[List[Str]] {
  let built = ["ok"]
  built
}

proc fail_explicitly() [error] -> Result[List[Str]] {
  return Err(ReturnError.Failure("explicit failure"))
}

proc fail_leaf() [error] -> Result[Int] {
  return Err(ReturnError.Failure("propagated failure"))
}

proc fail_through_question() [error] -> Result[Int] {
  fail_leaf()?
}

proc implicit_unit() [error] {
  test.eq(1, 1)?
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

proc test_implicit_result_return_in_par_map() [error] {
  let values = [1, 2]
    |> par-map --jobs=2 { |_|
      build()
    }

  test.eq(values, [["ok"], ["ok"]])?

  let block_values = [1, 2]
    |> par-map --jobs=2 { |_|
      let built = ["ok"]
      built
    }

  test.eq(block_values, [["ok"], ["ok"]])?
}

proc test_result_return_shapes_agree() [error] {
  test.eq(build()?, ["ok"])?
  implicit_unit()?

  match fail_explicitly() {
    Err(error) => test.eq(error.message, "explicit failure")?
    _ => test.ok(false)?
  }

  match fail_through_question() {
    Err(error) => test.eq(error.message, "propagated failure")?
    _ => test.ok(false)?
  }
}

proc test_nested_result_calls_in_par_map() [error] {
  let values = [1, 2]
    |> par-map --jobs=2 { |value|
      middle(value)
    }

  test.eq(values, [1, 2])?
}

proc test_result_alias_return_shape() [error] {
  test.eq(build_bare_through_alias()?, ["ok"])?
}

proc test_explicit_result_return_shapes(ctx: TestContext) [fs, error] {
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

  test.ok(output.success, output.stderr)?
  test.eq(
    output.stdout,
    """ok ok 3
""",
  )?
}

proc test_implicit_result_return_through_module(ctx: TestContext) [fs, error] {
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

let values = [1, 2] |> par-map --jobs=2 { |_|
  helper.build()
}
print values[0][0] values[1][0]
""",
    [],
    {XSH_MODULE_PATH: module_dir.display()},
  )?

  test.ok(output.status == 0, output.stderr)?
  test.eq(
    output.stdout,
    """ok ok
""",
  )?
}
