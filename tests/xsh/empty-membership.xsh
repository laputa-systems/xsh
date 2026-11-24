pure byte_in_empty(text: Str) -> Bool { (text.byte_at(0) ?? 0) in [] }
pure byte_not_in_empty(text: Str) -> Bool { (text.byte_at(0) ?? 0) not in [] }
pure int_in_empty(value: Int) -> Bool { value in [] }
pure text_in_empty(value: Str) -> Bool { value in [] }

test test_membership_in_an_empty_list_literal_is_false {
  assert ! byte_in_empty("x")
  assert ! byte_in_empty("")
  assert byte_not_in_empty("x")
  assert ! int_in_empty(3)
  assert ! text_in_empty("x")
}
