test test_full_inference_constructor_bodies_instantiate_int_and_str { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure box(value) { [value] }
pure succeed(value) { Ok(value) }
pure named(value) { {value: value} }
pure choose(flag, left, right) {
  if flag { left } else { right }
}
proc mark_int(label: Str, value: Int) [io] -> Result[Int] {
  print $label
  return Ok(value)
}
proc mark_text(label: Str, value: Str) [io] -> Result[Str] {
  print $label
  return Ok(value)
}
let integer_box = box(42)
let text_box = box("demo")
let integer_success = succeed(7)?
let text_success = succeed("ready")?
let integer_row = named(11)
let text_row = named("kept")
let integer_choice = choose(flag: true, right: mark_int("right-int", 40)?, left: mark_int("left-int", 42)?)
let text_choice = choose(flag: false, right: mark_text("right-text", "right")?, left: mark_text("left-text", "left")?)
assert integer_box == [42]
assert text_box == ["demo"]
assert integer_row.value == 11
assert text_row.value == "kept"
assert integer_choice == 42
assert text_choice == "right"
print ${integer_box[0]}
print ${text_box[0]}
print $integer_success
print $text_success
print ${integer_row.value}
print ${text_row.value}
print $integer_choice
print $text_choice
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "right-int\nleft-int\nright-text\nleft-text\n42\ndemo\n7\nready\n11\nkept\n42\nright\n"
}

test test_full_inference_generic_branch_local_prefix_with_inline_condition_is_lazy { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure ground_copy(value: Int) -> Int { value }
pure explode(divisor: Int) -> Int { 1 / divisor }
pure select(flag, value) {
  if flag {
    let observed: Int = ground_copy(1)
    if observed == 1 { value } else { value }
  } else {
    let observed: Int = explode(0)
    if observed == 1 { value } else { value }
  }
}
let integer = select(true, 42)
let text = select(true, "safe")
assert integer == 42
assert text == "safe"
print $integer
print $text
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\nsafe\n"
}

test test_full_inference_generic_branch_local_prefix_with_ground_condition_is_lazy { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure ground_copy(value: Int) -> Int { value }
pure explode(divisor: Int) -> Int { 1 / divisor }
pure is_one(value: Int) -> Bool { value == 1 }
pure select(flag, value) {
  if flag {
    let observed: Int = ground_copy(1)
    if is_one(observed) { value } else { value }
  } else {
    let observed: Int = explode(0)
    if is_one(observed) { value } else { value }
  }
}
let integer = select(true, 42)
let text = select(true, "safe")
assert integer == 42
assert text == "safe"
print $integer
print $text
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "42\nsafe\n"
}

test test_full_inference_box_rejects_a_conflicting_element_context { |ctx|
  test.expect(
    ctx,
    r"""pure box(value) { [value] }
let invalid: List[Str] = box(42)
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_result_constructor_rejects_a_conflicting_payload { |ctx|
  test.expect(
    ctx,
    r"""pure succeed(value) { Ok(value) }
let invalid: Result[Str] = succeed(42)
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_named_constructor_rejects_a_conflicting_field_context { |ctx|
  test.expect(
    ctx,
    r"""pure named(value) { {value: value} }
let invalid: Str = named(42).value
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}

test test_full_inference_conditional_constructor_rejects_incompatible_branches { |ctx|
  test.expect(
    ctx,
    r"""pure choose(flag: Bool, left: Int, right: Int) -> Int {
  if flag { left } else { right }
}
let invalid = choose(true, 42, "demo")
""",
    status: 2,
    stderr: ["[check.type-mismatch]"],
  )?
}
