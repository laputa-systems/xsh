test test_expr_binary_evaluates_left_then_right_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc operand(label: Str, value: Int) [io] -> Int {
  print $label
  return value
}
let total = operand("left", 2) + operand("right", 3)
assert total == 5
""",
    status: 0,
  )?
  assert output.stdout == """left
right
"""
}

test test_expr_fmt_string_evaluates_parts_left_to_right_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc part(label: Str) [io] -> Str {
  print $label
  return label
}
let rendered = f"{part("left")}|{part("right")}"
assert rendered == "left|right"
""",
    status: 0,
  )?
  assert output.stdout == """left
right
"""
}

test test_expr_path_fmt_string_evaluates_parts_left_to_right_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc part(label: Str) [io] -> Str {
  print $label
  return label
}
let formatted = fp"{part("left")}/{part("right")}"
assert formatted == p"left/right"
""",
    status: 0,
  )?
  assert output.stdout == """left
right
"""
}

test test_expr_glob_literal_has_no_child_and_expands_in_current_directory { |ctx|
  let root = test.temp_dir(ctx, name: "expr-glob-order")?
  fp"{root}/only.xsh".write("contents")?

  let output = test.expect(
    ctx,
    f"""let root = p"{root}"
cd root {{
  let matches = g"*.xsh"
  assert matches == [p"only.xsh"]
}}
""",
    status: 0,
  )?
  assert output.stdout == ""
}

test test_expr_last_status_reads_process_state_without_child_operands { |ctx|
  let output = test.expect(
    ctx,
    r"""run.status false
let status = $?
assert status.exited_with(1)
""",
    status: 0,
  )?
  assert output.stdout == ""
}

test test_expr_record_evaluates_fields_in_source_order_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc field(label: Str, value: Int) [io] -> Int {
  print $label
  return value
}
let item = {first: field("first", 1), second: field("second", 2)}
assert item.first == 1
assert item.second == 2
""",
    status: 0,
  )?
  assert output.stdout == """first
second
"""
}

test test_expr_map_literal_evaluates_each_key_before_its_value { |ctx|
  let output = test.expect(
    ctx,
    r"""proc key(label: Str) [io] -> Str {
  print $label
  return label
}
proc value(label: Str, number: Int) [io] -> Int {
  print $label
  return number
}
let entries = {[key("key-one")]: value("value-one", 1), [key("key-two")]: value("value-two", 2)}
assert (entries.get("key-one") ?? 0) == 1
assert (entries.get("key-two") ?? 0) == 2
""",
    status: 0,
  )?
  assert output.stdout == """key-one
value-one
key-two
value-two
"""
}

test test_expr_record_update_evaluates_base_then_updates_in_source_order { |ctx|
  let output = test.expect(
    ctx,
    r"""type Item = {name: Str, count: Int}
let seed: Item = {name: "old", count: 0}
proc base() [io] -> Item {
  print "base"
  return seed
}
proc name() [io] -> Str {
  print "name"
  return "new"
}
proc count() [io] -> Int {
  print "count"
  return 4
}
let updated = {...base(), name: name(), count: count()}
assert updated.name == "new"
assert updated.count == 4
""",
    status: 0,
  )?
  assert output.stdout == """base
name
count
"""
}

test test_expr_list_evaluates_elements_in_source_order_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc element(label: Str, value: Int) [io] -> Int {
  print $label
  return value
}
let values = [element("first", 1), element("second", 2)]
assert values == [1, 2]
""",
    status: 0,
  )?
  assert output.stdout == """first
second
"""
}

test test_expr_list_build_evaluates_spliced_segments_in_source_order_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc element(label: Str, value: Int) [io] -> Int {
  print $label
  return value
}
proc segment() [io] -> List[Int] {
  print "spread"
  return [2, 3]
}
let values = [element("first", 1), @segment(), element("last", 4)]
assert values == [1, 2, 3, 4]
""",
    status: 0,
  )?
  assert output.stdout == """first
spread
last
"""
}

test test_expr_range_evaluates_start_then_end_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc bound(label: Str, value: Int) [io] -> Int {
  print $label
  return value
}
let values = range(bound("start", 2), bound("end", 5)).collect()
assert values == [2, 3, 4]
""",
    status: 0,
  )?
  assert output.stdout == """start
end
"""
}

test test_expr_tag_evaluates_payloads_in_source_order_once { |ctx|
  let output = test.expect(
    ctx,
    r"""enum Choice { Pair(Str, Str) }
proc payload(label: Str) [io] -> Str {
  print $label
  return label
}
let selected = Pair(payload("first"), payload("second"))
match selected {
  Pair(first, second) => {
    assert first == "first"
    assert second == "second"
  }
}
""",
    status: 0,
  )?
  assert output.stdout == """first
second
"""
}

test test_expr_field_evaluates_receiver_once { |ctx|
  let output = test.expect(
    ctx,
    r"""type Item = {name: Str}
proc receiver() [io] -> Item {
  print "receiver"
  return {name: "selected"}
}
let name = receiver().name
assert name == "selected"
""",
    status: 0,
  )?
  assert output.stdout == "receiver\n"
}

test test_expr_index_evaluates_receiver_before_index_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc receiver() [io] -> List[Int] {
  print "receiver"
  return [10, 20]
}
proc index() [io] -> Int {
  print "index"
  return 1
}
let selected = receiver()[index()]
assert selected == 20
""",
    status: 0,
  )?
  assert output.stdout == """receiver
index
"""
}

test test_expr_index_from_end_evaluates_receiver_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc receiver() [io] -> List[Str] {
  print "receiver"
  return ["first", "last"]
}
let selected = receiver()[-1]
assert selected == "last"
""",
    status: 0,
  )?
  assert output.stdout == "receiver\n"
}

test test_expr_slice_evaluates_receiver_then_present_bounds_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc receiver() [io] -> List[Int] {
  print "receiver"
  return [10, 20, 30, 40]
}
proc bound(label: Str, value: Int) [io] -> Int {
  print $label
  return value
}
let selected = receiver()[bound("start", 1)..bound("end", 3)]
assert selected == [20, 30]
""",
    status: 0,
  )?
  assert output.stdout == """receiver
start
end
"""
}

test test_expr_str_byte_len_evaluates_receiver_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc receiver() [io] -> Str {
  print "receiver"
  return "é"
}
assert receiver().byte_len() == 2
""",
    status: 0,
  )?
  assert output.stdout == "receiver\n"
}

test test_expr_str_byte_at_evaluates_receiver_then_index_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc receiver() [io] -> Str {
  print "receiver"
  return "x"
}
proc index() [io] -> Int {
  print "index"
  return 0
}
let selected = receiver().byte_at(index())
assert selected == 120
""",
    status: 0,
  )?
  assert output.stdout == """receiver
index
"""
}

test test_expr_str_predicate_evaluates_receiver_then_needle_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc receiver() [io] -> Str {
  print "receiver"
  return "alpha"
}
proc needle() [io] -> Str {
  print "needle"
  return "al"
}
assert receiver().starts_with(needle())
""",
    status: 0,
  )?
  assert output.stdout == """receiver
needle
"""
}

test test_expr_require_evaluates_value_once { |ctx|
  let output = test.expect(
    ctx,
    r"""proc value() [io] -> Any {
  print "value"
  return "checked"
}
let checked = value().require(Str)?
assert checked == "checked"
""",
    status: 0,
  )?
  assert output.stdout == "value\n"
}

test test_expr_checked_value_evaluates_constrained_value_once { |ctx|
  let output = test.expect(
    ctx,
    r"""error CountError = Invalid(count: UInt)
proc count() [io] -> UInt {
  print "count"
  return 7
}
match Err(CountError.Invalid(count())) {
  Err(CountError.Invalid {count}) => assert count == 7
  Err(error) => fail error.message
  Ok(_) => fail "expected an error"
}
""",
    status: 0,
  )?
  assert output.stdout == "count\n"
}
