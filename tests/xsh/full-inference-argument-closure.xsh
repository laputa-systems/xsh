test test_full_inference_generic_arguments_close_byte_at_and_optional_fallback { |ctx|
  let checks = r"""
assert blanks(b"a b\t") == [false, true, false, true, false]
assert first_blank("a b")
assert raw_byte(b"") == null
assert raw_byte(b"A") == 65
print "closed"
"""
  let inferred = test.run_xsh(ctx, r"""
pure is_blank(value) -> Bool { value == 32 or value == 9 }
pure blanks(data: Bytes) -> List[Bool] {
  [is_blank(data.byte_at(index) ?? 0) for index in range(data.len() + 1)]
}
pure first_blank(text: Str) -> Bool { is_blank(text.byte_at(1) ?? 0) }
pure same(value) { value }
pure raw_byte(data: Bytes) { same(data.byte_at(0)) }
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
pure is_blank(value: Int) -> Bool { value == 32 or value == 9 }
pure blanks(data: Bytes) -> List[Bool] {
  [is_blank(data.byte_at(index) ?? 0) for index in range(data.len() + 1)]
}
pure first_blank(text: Str) -> Bool { is_blank(text.byte_at(1) ?? 0) }
pure same(value: Int?) -> Int? { value }
pure raw_byte(data: Bytes) -> Int? { same(data.byte_at(0)) }
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "closed\n"
}

test test_full_inference_record_arguments_close_mutated_locals_beside_empty_lists { |ctx|
  let checks = r"""
pure width(source: Int) -> Int {
  var total = source
  total = total + 1
  describe(source, {total: total, items: []})
}
assert width(4) == 5
print "closed"
"""
  let inferred = test.run_xsh(ctx, r"""
type Entry = {total: Int, items: List[Int]}
pure describe(source, entry: Entry) -> Int { entry.total + entry.items.len() }
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
type Entry = {total: Int, items: List[Int]}
pure describe(source: Int, entry: Entry) -> Int { entry.total + entry.items.len() }
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "closed\n"
}

test test_full_inference_optional_local_assignment_from_generic_element { |ctx|
  let checks = r"""
assert pick(["a", "b"], 1) == "b"
assert pick(["a"], 3) == null
print "assigned"
"""
  let inferred = test.run_xsh(ctx, r"""
pure pick(items, index: Int) -> Str? {
  var found: Str? = null
  if index < items.len() { found = items[index] }
  found
}
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
pure pick(items: List[Str], index: Int) -> Str? {
  var found: Str? = null
  if index < items.len() { found = items[index] }
  found
}
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "assigned\n"
}

test test_full_inference_optional_local_assignment_rejects_other_element_domains { |ctx|
  test.expect(ctx, r"""
pure pick(items, index: Int) -> Str? {
  var found: Str? = null
  if index < items.len() { found = items[index] }
  found
}
let rejected = pick([1, 2], 0)
""", status: 2, stderr: ["[check.callable-mismatch]"])?
}

test test_full_inference_generic_body_generalizes_past_a_returning_match_arm { |ctx|
  let checks = r"""
assert classify(1, "x") == "one"
assert classify(2, "x") == "other"
assert classify(7, "x") == "returned"
print "generalized"
"""
  let inferred = test.run_xsh(ctx, r"""
pure classify(code: Int, tag) -> Str {
  let name = match code {
    1 => "one"
    2 => "other"
    else => { return "returned" }
  }
  let _ = tag == "x"
  name
}
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
pure classify(code: Int, tag: Str) -> Str {
  let name = match code {
    1 => "one"
    2 => "other"
    else => { return "returned" }
  }
  let _ = tag == "x"
  name
}
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "generalized\n"
}
