test test_int_int { |ctx|
  test.expect(
    ctx,
    r"""let answer: Int = 42
assert answer == 42
""",
    status: 0,
  )?
}

test test_int_slot { |ctx|
  test.expect(
    ctx,
    r"""pure identity(value: Int) -> Int {
  let result: Int = value + 0
  return result
}
assert identity(42) == 42
""",
    status: 0,
  )?
}

test test_int_binary { |ctx|
  test.expect(
    ctx,
    r"""pure subtract(left: Int, right: Int) -> Int {
  let result: Int = left - right
  return result
}
assert subtract(20, 3) == 17
""",
    status: 0,
  )?
  test.expect(
    ctx,
    r"""pure add(left: Int, right: Int) -> Int {
  let result: Int = left + right
  return result
}
print ${add(9223372036854775807, 1)}
""",
    status: 3,
    stderr: ["integer-overflow"],
  )?
}

test test_int_str_byte_len_slot { |ctx|
  test.expect(
    ctx,
    r"""pure byte_len(text: Str) -> Int {
  let result: Int = text.byte_len()
  return result
}
assert byte_len("é") == 2
""",
    status: 0,
  )?
}

test test_int_str_count_lines_slot { |ctx|
  test.expect(
    ctx,
    r"""pure count_lines(text: Str) -> Int {
  let result: Int = text.count_lines()
  return result
}
assert count_lines("a\nb\n") == 2
""",
    status: 0,
  )?
}

test test_int_str_byte_at_slot { |ctx|
  test.expect(
    ctx,
    r"""pure byte_or(text: Str, index: Int) -> Int {
  let result: Int = text.byte_at(index) ?? 91
  return result
}
assert byte_or("é", 0) == 195
assert byte_or("é", 1) == 169
assert byte_or("é", 2) == 91

proc lazy_default() [] -> Int {
  print "default"
  return 91
}
let present = "x".byte_at(0) ?? lazy_default()
assert present == 120
let absent = "x".byte_at(1) ?? lazy_default()
assert absent == 91
""",
    status: 0,
    stdout: ["default\n"],
  )?
}

test test_bool_bool { |ctx|
  test.expect(
    ctx,
    r"""let answer: Bool = true
assert answer
""",
    status: 0,
  )?
}

test test_bool_slot { |ctx|
  test.expect(
    ctx,
    r"""pure identity(value: Bool) -> Bool {
  let result: Bool = value and true
  return result
}
assert identity(true)
assert !identity(false)
""",
    status: 0,
  )?
}

test test_bool_not { |ctx|
  test.expect(
    ctx,
    r"""pure absent(values: List[Int]) -> Bool {
  let result: Bool = 3 not in values
  return result
}
assert absent([1, 4, 5])
assert !absent([1, 3, 5])
""",
    status: 0,
  )?
}

test test_bool_and { |ctx|
  test.expect(
    ctx,
    r"""pure guarded(left: Bool) -> Bool {
  let result: Bool = left and 1 / 0 == 0
  return result
}
assert !guarded(false)
""",
    status: 0,
  )?
  test.expect(
    ctx,
    r"""pure guarded(left: Bool) -> Bool {
  let result: Bool = left and 1 / 0 == 0
  return result
}
print ${guarded(true)}
""",
    status: 3,
    stderr: ["division-by-zero"],
  )?
}

test test_bool_or { |ctx|
  test.expect(
    ctx,
    r"""pure guarded(left: Bool) -> Bool {
  let result: Bool = left or 1 / 0 == 0
  return result
}
assert guarded(true)
""",
    status: 0,
  )?
  test.expect(
    ctx,
    r"""pure guarded(left: Bool) -> Bool {
  let result: Bool = left or 1 / 0 == 0
  return result
}
print ${guarded(false)}
""",
    status: 3,
    stderr: ["division-by-zero"],
  )?
}

test test_bool_int_compare { |ctx|
  test.expect(
    ctx,
    r"""pure greater(left: Int, right: Int) -> Bool {
  let result: Bool = left + 0 > right + 0
  return result
}
assert greater(9, 4)
assert !greater(4, 9)
""",
    status: 0,
  )?
}

test test_bool_str_predicate_slot { |ctx|
  test.expect(
    ctx,
    r"""pure has_prefix(text: Str) -> Bool {
  let result: Bool = text.starts_with("alpha")
  return result
}
assert has_prefix("alphabet")
assert !has_prefix("betaalpha")
""",
    status: 0,
  )?
}

test test_bool_contains_slot { |ctx|
  test.expect(
    ctx,
    r"""pure has_value(values: List[Int]) -> Bool {
  let result: Bool = 3 in values
  return result
}
assert has_value([1, 3, 5])
assert !has_value([1, 4, 5])
""",
    status: 0,
  )?
}

test test_bool_str_contains_slot { |ctx|
  test.expect(
    ctx,
    r"""pure has_fragment(text: Str) -> Bool {
  let result: Bool = "pha" in text
  return result
}
assert has_fragment("alphabet")
assert !has_fragment("beta")
""",
    status: 0,
  )?
}

test test_bool_trim_empty_slot { |ctx|
  test.expect(
    ctx,
    r"""pure is_blank(text: Str) -> Bool {
  let result: Bool = text.trim() == ""
  return result
}
assert is_blank(" \t\n")
assert !is_blank(" x ")
""",
    status: 0,
  )?
}

test test_bool_trim_str_predicate_slot { |ctx|
  test.expect(
    ctx,
    r"""pure has_trimmed_prefix(text: Str) -> Bool {
  let result: Bool = text.trim().starts_with("alpha")
  return result
}
assert has_trimmed_prefix("  alphabet  ")
assert !has_trimmed_prefix("  beta  ")
""",
    status: 0,
  )?
}

test test_bool_literal_compare_slot { |ctx|
  test.expect(
    ctx,
    r"""pure is_ready(text: Str) -> Bool {
  let result: Bool = text == "ready"
  return result
}
assert is_ready("ready")
assert !is_ready("waiting")
""",
    status: 0,
  )?
}

test test_expr_null { |ctx|
  test.expect(
    ctx,
    r"""let values: List[Any] = [null]
assert values[0] is Null
""",
    status: 0,
  )?
}

test test_expr_unit { |ctx|
  test.expect(
    ctx,
    r"""pure unit() -> Unit {}
let value: Any = unit()
assert value is Unit
""",
    status: 0,
  )?
}

test test_expr_int { |ctx|
  let output = test.expect(
    ctx,
    r"""let value: Any = 42
print ${value.require(Int)?}
""",
    status: 0,
  )?
  assert output.stdout == "42\n"
}

test test_expr_float { |ctx|
  let output = test.expect(
    ctx,
    r"""let values: List[Any] = [1.5]
print ${values[0].require(Float)?}
""",
    status: 0,
  )?
  assert output.stdout == "1.5\n"
}

test test_expr_duration { |ctx|
  let output = test.expect(
    ctx,
    r"""let values: List[Any] = [2s]
print ${values[0].require(Duration)?}
""",
    status: 0,
  )?
  assert output.stdout == "2s\n"
}

test test_expr_bool { |ctx|
  test.expect(
    ctx,
    "let values: List[Any] = [true]\nassert values[0].require(Bool)?\n",
    status: 0,
  )?
}

test test_expr_str { |ctx|
  let output = test.expect(
    ctx,
    r"""let values: List[Any] = ["route"]
print ${values[0].require(Str)?}
""",
    status: 0,
  )?
  assert output.stdout == "route\n"
}

test test_expr_bytes { |ctx|
  test.expect(
    ctx,
    r"""let values: List[Any] = [b"route"]
assert values[0].require(Bytes)? == bytes.from_text("route")
""",
    status: 0,
  )?
}

test test_expr_prepared_regex { |ctx|
  test.expect(
    ctx,
    r"""let pattern = rx"^route$"
assert pattern.matches("route")
assert !pattern.matches("router")
""",
    status: 0,
  )?
}

test test_expr_prepared_constant { |ctx|
  test.expect(
    ctx,
    "const ANSWER = 40 + 2",
    status: 0,
  )?
}

test test_prepared_constant_value_remains_42 { |ctx|
  let output = test.run_script(
    ctx,
    r"""const ANSWER = 40 + 2
let values: List[Any] = [ANSWER]
assert values[0].require(Int)? == 42
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == ""
}

test test_expr_path { |ctx|
  test.expect(
    ctx,
    r"""let values: List[Any] = [p"route/oracle"]
assert values[0].require(Path)?.display() == "route/oracle"
""",
    status: 0,
  )?
}

test test_expr_param { |ctx|
  test.expect(
    ctx,
    r"""pure identity(value: Duration) -> Duration { value }
assert identity(7s) == 7s
""",
    status: 0,
  )?
}
