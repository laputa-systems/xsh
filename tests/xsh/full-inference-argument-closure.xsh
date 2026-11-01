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
