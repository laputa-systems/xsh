test test_operand_order_int_binary_is_left_to_right { |ctx|
  let values = test.expect(
    ctx,
    r"""let value: Int = (10 - 3) * (8 - 5)
print $value
""",
    status: 0,
  )?
  assert values.stdout == "21\n"

  let first_failure = test.expect(
    ctx,
    r"""let value: Int = 9223372036854775807 + 1 + 1 / 0
print $value
""",
    status: 3,
    stderr: ["integer-overflow"],
  )?
  assert "division-by-zero" not in first_failure.stderr, first_failure.stderr
}

test test_operand_order_int_byte_at_uses_index_and_static_default { |ctx|
  let values = test.expect(
    ctx,
    r"""pure byte_after(text: Str, index: Int) -> Int {
  let result: Int = text.byte_at(index + 1) ?? 77
  return result
}
let present = byte_after("abc", 0)
let absent = byte_after("abc", 2)
print f"{present} {absent}"
""",
    status: 0,
  )?
  assert values.stdout == "98 77\n"

  test.expect(
    ctx,
    r"""pure byte_after(text: Str, index: Int) -> Int {
  let result: Int = text.byte_at(index + 1) ?? 77
  return result
}
print ${byte_after("abc", 9223372036854775807)}
""",
    status: 3,
    stderr: ["integer-overflow"],
  )?
}

test test_operand_order_bool_not_uses_its_membership_value { |ctx|
  test.expect(
    ctx,
    r"""pure absent(values: List[Int]) -> Bool {
  let result: Bool = 3 not in values
  return result
}
assert absent([1, 2])
assert !absent([3])
""",
    status: 0,
  )?
}

test test_operand_order_bool_and_skips_or_reaches_right { |ctx|
  test.expect(
    ctx,
    r"""pure conjunction(left: Bool) -> Bool {
  let result: Bool = left and 1 / 0 == 0
  return result
}
assert !conjunction(false)
""",
    status: 0,
  )?

  test.expect(
    ctx,
    r"""pure conjunction(left: Bool) -> Bool {
  let result: Bool = left and 1 / 0 == 0
  return result
}
print ${conjunction(true)}
""",
    status: 3,
    stderr: ["division-by-zero"],
  )?

  let first_failure = test.expect(
    ctx,
    r"""pure conjunction() -> Bool {
  let result: Bool = 9223372036854775807 + 1 > 0 and 1 / 0 == 0
  return result
}
print ${conjunction()}
""",
    status: 3,
    stderr: ["integer-overflow"],
  )?
  assert "division-by-zero" not in first_failure.stderr, first_failure.stderr
}

test test_operand_order_bool_or_skips_or_reaches_right { |ctx|
  test.expect(
    ctx,
    r"""pure disjunction(left: Bool) -> Bool {
  let result: Bool = left or 1 / 0 == 0
  return result
}
assert disjunction(true)
""",
    status: 0,
  )?

  test.expect(
    ctx,
    r"""pure disjunction(left: Bool) -> Bool {
  let result: Bool = left or 1 / 0 == 0
  return result
}
print ${disjunction(false)}
""",
    status: 3,
    stderr: ["division-by-zero"],
  )?

  let first_failure = test.expect(
    ctx,
    r"""pure disjunction() -> Bool {
  let result: Bool = 9223372036854775807 + 1 > 0 or 1 / 0 == 0
  return result
}
print ${disjunction()}
""",
    status: 3,
    stderr: ["integer-overflow"],
  )?
  assert "division-by-zero" not in first_failure.stderr, first_failure.stderr
}

test test_operand_order_bool_int_compare_is_left_to_right { |ctx|
  let values = test.expect(
    ctx,
    r"""let greater = 9 + 1 > 4 + 2
print $greater
""",
    status: 0,
  )?
  assert values.stdout == "true\n"

  let first_failure = test.expect(
    ctx,
    r"""let greater = 9223372036854775807 + 1 > 1 / 0
print $greater
""",
    status: 3,
    stderr: ["integer-overflow"],
  )?
  assert "division-by-zero" not in first_failure.stderr, first_failure.stderr
}

test test_operand_order_path_from_evaluates_its_value_once { |ctx|
  let values = test.expect(
    ctx,
    r"""proc source() [io] -> Str {
  print "source"
  return "folder/file"
}
let candidate: Path = Path(source())
assert candidate == p"folder/file"
print f"{candidate}"
""",
    status: 0,
  )?
  assert values.stdout == "source\nfolder/file\n"
}
