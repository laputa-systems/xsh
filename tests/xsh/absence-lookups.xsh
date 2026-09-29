test test_absence_lookup_find_preserves_zero_byte_offsets_and_empty_needles [error] {
  test.eq("a:b".find("a"), 0)?
  test.eq("a:b".find(":"), 1)?
  test.eq("é:x".find(":"), 2)?
  test.eq("a:b".find("missing"), null)?
  test.eq("a:b".find("", 3), 3)?
  test.eq("a:b".find("", 4), null)?
  test.eq("a:b".find("a", -1), null)?
  test.eq("a:b".find("b", 2), 2)?
  test.eq("".find(""), 0)?
  test.eq("".find("x"), null)?
  test.eq("a:b".find(":", start: 0), 1)?
}

test test_absence_lookup_byte_at_returns_null_outside_byte_domain [error] {
  test.eq("é".byte_at(index: 0), 195)?
  test.eq("é".byte_at(1), 169)?
  test.eq("é".byte_at(2), null)?
  test.eq("é".byte_at(-1), null)?
  test.eq(b"\x00\xff".byte_at(0), 0)?
  test.eq(b"\x00\xff".byte_at(1), 255)?
  test.eq(b"\x00\xff".byte_at(2), null)?
  test.eq(b"\x00\xff".byte_at(-1), null)?
}

test test_absence_lookup_collection_get_preserves_present_null_and_typed_errors [error] {
  let values: List[Int?] = [null, 3]
  let entries: Map[Int?] = {present: null, count: 3}
  test.eq(values.get(0) ?? 7, null)?
  test.eq(values.get(2) ?? 7, 7)?
  test.eq(entries.get("present") ?? 7, null)?
  test.eq(entries.get("missing") ?? 7, 7)?
  test.error_kind(values.get(2), "index-out-of-bounds")?
  test.error_kind(entries.get("missing"), "map-missing")?
}

test test_absence_lookup_removed_overloads_are_rejected [error] { |ctx|
  for source in ["let value = [1].get(0, 7)", "let value = {one: 1}.get(\"one\", 7)", "let value = \"a\".byte_at(0, -1)", "let value = b\"a\".byte_at(0, -1)", "let value = [1].get(index: 0, fallback: 7)", "let value = \"a\".byte_at(index: 0, default: 7)"] {
    let invalid = test.run_script(ctx, source)?
    test.ok(!invalid.success, invalid.stderr)?
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

test test_absence_lookup_nullable_and_integer_fast_paths_agree [error] {
  for index in [-1, 0, 1, 2, 9223372036854775807] {
    test.eq(absence_lookup_nullable_byte("é", index), "é".byte_at(index))?
    test.eq(absence_lookup_integer_byte("é", index), "é".byte_at(index) ?? -1)?
  }
  test.eq("é:x".find(":", 1), 2)?
  test.eq("é:x".find("é", 1), null)?
}

test test_absence_lookup_lazy_fallback_and_authored_eager_snapshots [error] { |ctx|
  let output = test.run_script(ctx, r"""proc receiver() [io] -> List[Int] { print "receiver"; [3] }
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
""")?
  test.eq(output.success, true)?
  test.eq(output.stdout, "receiver\nindex\n3\nreceiver\nindex\nfallback\n3\nfallback\n7\n97\nfallback\n7\n")?
}

test test_absence_lookup_eager_snapshots_keep_receiver_and_index_before_mutation [error] { |ctx|
  let output = test.run_script(ctx, r"""proc witness() [io] -> Int {
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
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "receiver\nindex\nfallback\n9 1\n3\n")?
}
