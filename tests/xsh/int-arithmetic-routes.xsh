pure typed_route(op: Str, left: Int, right: Int) -> Int {
  if op == "+" {
    let result = left + right
    return result
  } else if op == "-" {
    let result = left - right
    return result
  } else if op == "*" {
    let result = left * right
    return result
  } else if op == "/" {
    let result = left / right
    return result
  }
  let result = left % right
  result
}

pure returned_route(op: Str, left: Int, right: Int) -> Int {
  if op == "+" {
    return left + right
  } else if op == "-" {
    return left - right
  } else if op == "*" {
    return left * right
  } else if op == "/" {
    return left / right
  }
  left % right
}

pure compound_route(op: Str, left: Int, right: Int) -> Int {
  var value = left
  if op == "+" {
    value += right
  } else if op == "-" {
    value -= right
  } else if op == "*" {
    value *= right
  } else if op == "/" {
    value /= right
  } else {
    value %= right
  }
  value
}

const MIN_INT = 0 - 9223372036854775807 - 1
const MAX_INT = 9223372036854775807
const FOLDED_QUOTIENT = (0 - 7) / 2
const FOLDED_REMAINDER = (0 - 7) % 2
const FOLDED_MIRROR = 7 % (0 - 2)

test test_int_routes_truncate_toward_zero {
  for route in ["typed", "returned", "compound"] {
    for sample in [
      {op: "/", left: -7, right: 2, expected: -3},
      {op: "%", left: -7, right: 2, expected: -1},
      {op: "/", left: 7, right: -2, expected: -3},
      {op: "%", left: 7, right: -2, expected: 1},
      {op: "/", left: MIN_INT, right: 2, expected: -4611686018427387904},
      {op: "%", left: MIN_INT, right: 1, expected: 0},
      {op: "%", left: MAX_INT, right: -1, expected: 0},
      {op: "+", left: MAX_INT, right: MIN_INT, expected: -1},
      {op: "*", left: MIN_INT, right: 1, expected: MIN_INT},
    ] {
      let actual = if route == "typed" {
        typed_route(sample.op, sample.left, sample.right)
      } else if route == "returned" {
        returned_route(sample.op, sample.left, sample.right)
      } else {
        compound_route(sample.op, sample.left, sample.right)
      }
      assert actual == sample.expected, f"{route} {sample.left} {sample.op} {sample.right} = {actual}"
    }
  }
}

test test_top_level_compound_assignment_truncates_toward_zero { |ctx|
  # A top-level `name OP= value` is a driver assignment; its failures are the
  # compound cases of test_int_routes_report_each_failure_at_its_operator.
  let output = test.expect(
    ctx,
    "var sum = 9223372036854775807\nsum += -9223372036854775807 - 1\nvar difference = 5\ndifference -= 7\nvar product = -9223372036854775807 - 1\nproduct *= 1\nvar quotient = -7\nquotient /= 2\nvar remainder = -7\nremainder %= 2\nvar mirrored = 7\nmirrored %= -2\nprint f\"{sum} {difference} {product} {quotient} {remainder} {mirrored}\"\n",
    status: 0,
  )?
  assert output.stdout == "-1 -2 -9223372036854775808 -3 -1 1\n", output.stdout
}

test test_int_constants_fold_truncating_division {
  assert FOLDED_QUOTIENT == -3
  assert FOLDED_REMAINDER == -1
  assert FOLDED_MIRROR == 1
  assert (0 - 7) / 2 == -3 and 7 / (0 - 2) == -3
}

pure literal_operand(op: Str, operand: Str) -> Str {
  if op in ["*", "/", "%"] and " " in operand {
    return "(" + operand + ")"
  }
  operand
}

test test_int_routes_report_each_failure_at_its_operator { |ctx|
  for failure in [
    {op: "+", left: "9223372036854775807", right: "1", code: "integer-overflow", message: "integer overflow"},
    {op: "-", left: "0 - 9223372036854775807", right: "2", code: "integer-overflow", message: "integer overflow"},
    {op: "*", left: "9223372036854775807", right: "2", code: "integer-overflow", message: "integer overflow"},
    {op: "/", left: "7", right: "0", code: "division-by-zero", message: "division by zero"},
    {op: "%", left: "7", right: "0", code: "division-by-zero", message: "division by zero"},
    {op: "/", left: "0 - 9223372036854775807 - 1", right: "0 - 1", code: "integer-overflow", message: "integer overflow"},
    {op: "%", left: "0 - 9223372036854775807 - 1", right: "0 - 1", code: "integer-overflow", message: "integer overflow"},
  ] {
    let diagnostic = "err: " + failure.code + ": " + failure.message
    let call = r"print ${apply(" + failure.left + ", " + failure.right + ")}\n"
    let typed = test.expect(
      ctx,
      "pure apply(left: Int, right: Int) -> Int {\n  let result = left " + failure.op + " right\n  return result\n}\n" + call,
      status: 3,
      stderr: [diagnostic],
    )?
    assert typed.stdout == "", typed.stdout
    let annotated = test.expect(
      ctx,
      "pure apply(left: Int, right: Int) -> Int {\n  let result: Int = left " + failure.op + " right\n  return result\n}\n" + call,
      status: 3,
      stderr: [diagnostic, ":2:21"],
    )?
    assert annotated.stdout == "", annotated.stdout
    let returned = test.expect(
      ctx,
      "pure apply(left: Int, right: Int) -> Int {\n  left " + failure.op + " right\n}\n" + call,
      status: 3,
      stderr: [diagnostic, ":2:3"],
    )?
    assert returned.stdout == "", returned.stdout
    let compound = test.expect(
      ctx,
      "var value = " + failure.left + "\nvalue " + failure.op + "= " + failure.right + "\nprint \$value\n",
      status: 3,
      stderr: [diagnostic, ":2:1"],
    )?
    assert compound.stdout == "", compound.stdout
    let expression = literal_operand(failure.op, failure.left) + " " + failure.op + " " + literal_operand(failure.op, failure.right)
    let literal = test.expect(
      ctx,
      "print \"before\"\n" + r"print ${" + expression + "}\n",
      status: 3,
      stderr: [diagnostic, ":2:9"],
    )?
    assert literal.stdout == "before\n", literal.stdout
    let constant = test.expect(
      ctx,
      "const BAD = " + expression + "\nprint \"ran\"\n",
      status: 2,
      stderr: ["err[check.const]: invalid constant operation or arithmetic overflow", ":1:13"],
    )?
    assert constant.stdout == "", constant.stdout
  }
}
