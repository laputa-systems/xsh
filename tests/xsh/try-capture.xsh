proc test_try_captures_once_and_preserves_result_data(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """
var calls = 0
proc operation() -> Result[Int] {
  calls += 1
  Ok(7)
}
let value = try { operation()? }?
let outer = try { operation() }?
let nested = outer?
print f"\${value} \${nested} \${calls}"
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "7 7 2\n")?
}
