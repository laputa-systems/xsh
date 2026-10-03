test test_absence_lookup_find_preserves_zero_byte_offsets_and_empty_needles {
  assert "a:b".find("a") == 0
  assert "a:b".find(":") == 1
  assert "é:x".find(":") == 2
  assert "a:b".find("missing") == null
  assert "a:b".find("", 3) == 3
  assert "a:b".find("", 4) == null
  assert "a:b".find("a", -1) == null
  assert "a:b".find("b", 2) == 2
  assert "".find("") == 0
  assert "".find("x") == null
  assert "a:b".find(":", start: 0) == 1
}

test test_absence_lookup_byte_at_returns_null_outside_byte_domain {
  assert "é".byte_at(index: 0) == 195
  assert "é".byte_at(1) == 169
  assert "é".byte_at(2) == null
  assert "é".byte_at(-1) == null
  assert b"\0\xff".byte_at(0) == 0
  assert b"\0\xff".byte_at(1) == 255
  assert b"\0\xff".byte_at(2) == null
  assert b"\0\xff".byte_at(-1) == null
}

test test_absence_lookup_collection_get_preserves_present_null_and_typed_errors {
  let values: List[Int?] = [null, 3]
  let entries: Map[Int?] = {present: null, count: 3}
  assert (values.get(0) ?? 7) == null
  assert (values.get(2) ?? 7) == 7
  assert (entries.get("present") ?? 7) == null
  assert (entries.get("missing") ?? 7) == 7
  test.error_kind(values.get(2), "index-out-of-bounds")?
  test.error_kind(entries.get("missing"), "map-missing")?
}

test test_absence_lookup_removed_overloads_are_rejected { |ctx|
  for source in [
    "let value = [1].get(0, 7)",
    "let value = {one: 1}.get(\"one\", 7)",
    "let value = \"a\".byte_at(0, -1)",
    "let value = b\"a\".byte_at(0, -1)",
    "let value = [1].get(index: 0, fallback: 7)",
    "let value = \"a\".byte_at(index: 0, default: 7)",
  ] {
    let invalid = test.run_script(ctx, source)?
    assert ! invalid.success, invalid.stderr
  }
}

pure absence_lookup_nullable_byte(text: Str, index: Int) -> Int? {
  let byte = text.byte_at(index)
  byte
}

pure absence_lookup_integer_byte(text: Str, index: Int) -> Int {
  let byte = text.byte_at(index) ?? -1
  byte
}

test test_absence_lookup_nullable_and_integer_fast_paths_agree {
  for index in [-1, 0, 1, 2, 9223372036854775807] {
    assert absence_lookup_nullable_byte("é", index) == "é".byte_at(index)
    assert absence_lookup_integer_byte("é", index) == ("é".byte_at(index) ?? -1)
  }

  assert "é:x".find(":", 1) == 2
  assert "é:x".find("é", 1) == null
}

test test_absence_lookup_lazy_fallback_and_authored_eager_snapshots { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc receiver() [io] -> List[Int] { print "receiver"; [3] }
proc index() [io] -> Int { print "index"; 0 }
proc fallback() [io] -> Int { print "fallback"; 7 }
let lazy = receiver().get(index()) ?? fallback()
print ${lazy}
let eager = {
  let values = receiver()
  let position = index()
  let alternative = fallback()
  values.get(position) ?? alternative
}
print ${eager}
let absent = [3].get(9) ?? fallback()
print ${absent}
let byte = "a".byte_at(0) ?? fallback()
print ${byte}
let missing_byte = b"a".byte_at(1) ?? fallback()
print ${missing_byte}
""",
  )?
  assert output.success
  assert output.stdout == """receiver
index
3
receiver
index
fallback
3
fallback
7
97
fallback
7
"""
}

test test_absence_lookup_eager_snapshots_keep_receiver_and_index_before_mutation { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc witness() [io] -> Int {
  var values = [3]
  var selected = 0
  let items = { print "receiver"; values }
  let position = { print "index"; selected }
  let alternative = { print "fallback"; values = [9]; selected = 1; 7 }
  let answer = items.get(position) ?? alternative
  print ${values[0]} ${selected}
  answer
}
print ${witness()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """receiver
index
fallback
9 1
3
"""
}
