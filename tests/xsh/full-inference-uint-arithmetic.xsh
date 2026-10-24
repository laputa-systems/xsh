test test_inferred_uint_arithmetic_preserves_intrinsic_results { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type Parts = {high: UInt, low: UInt}
pure returned(value) -> UInt { value / 2 }
pure stored(value) -> UInt { let whole: UInt = value / 2; whole }
pure aliased(value) -> UInt { let raw = value / 2; let whole: UInt = raw; whole }
pure parts(value) -> Parts { {high: value / 2, low: value % 2} }
pure divide(left, right) -> UInt { left / right }
assert returned(9) == 4
assert stored(9) == 4
assert aliased(9) == 4
assert parts(9) == {high: 4, low: 1}
assert divide(8s, 2s) == 4
proc closing(value, divisor: Int) [io, error] -> UInt {
  defer { print "clean" }
  value / divisor
}
assert closing(9, 2) == 4
""",
  )?
  assert output.status == 0, output.stderr
  assert output.stdout == "clean\n"
  assert output.stderr == ""
}

test test_inferred_uint_arithmetic_preserves_failures_and_cleanup { |ctx|
  for parameter in ["value: Int", "value"] {
    for scenario in [
      {arguments: "-9, 2", diagnostic: "err: type-error: return violates UInt constraint in UInt"},
      {arguments: "9, 0", diagnostic: "err: division-by-zero: division by zero"},
      {arguments: "-9223372036854775807 - 1, -1", diagnostic: "err: integer-overflow: integer overflow"},
    ] {
      let source = "proc divide(" + parameter + r""", divisor: Int) [io, error] -> UInt {
  defer { print "clean" }
  value / divisor
}
""" + "divide(" + scenario.arguments + ")\n"
      let output = test.expect(ctx, source, status: 3, stderr: [scenario.diagnostic])?
      assert output.stdout == "clean\n"
    }
  }
}

test test_inferred_uint_aliases_validate_at_the_later_boundary { |ctx|
  for parameter in ["value: Int", "value"] {
    let source = "proc divide(" + parameter + r""", divisor: Int) [io, error] -> UInt {
  defer { print "clean" }
  let raw = value / divisor
  print (raw)
  let whole: UInt = raw
  whole
}
divide(-9, 2)
"""
    let output = test.expect(
      ctx,
      source,
      status: 3,
      stderr: ["err: type-error: value violates UInt constraint in UInt", "let whole: UInt = raw"],
    )?
    assert output.stdout == "-4\nclean\n"
  }
}
