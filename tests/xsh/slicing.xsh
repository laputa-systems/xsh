test test_slice_existing_list_and_unicode_scalar_normalization [error] {
  let values = [10, 20, 30, 40]
  (values[1..3]) == ([20, 30])
  (values[..2]) == ([10, 20])
  (values[2..]) == ([30, 40])
  (values[..]) == (values)
  (values[-3..-1]) == ([20, 30])
  (values[-99..99]) == (values)
  (values[3..1]) == ([])
  (values[99..]) == ([])
  (values[..-99]) == ([])
  (values[2..2]) == ([])
  ([1][..0]) == ([])
  let empty: List[Int] = []
  (empty[-1..99]) == (empty)
  let text = "aé🦀e\u{301}z"
  (text[1..3]) == ("é🦀")
  (text[-3..-1]) == ("e\u{301}")
  (text[..]) == (text)
  (text[-99..99]) == (text)
  (text[3..1]) == ("")
  (text[99..]) == ("")
  (text[..-99]) == ("")
  (""[-1..99]) == ("")
}

test test_slice_bytes_normalization_and_nested_views [error] { |ctx|
  let output = test.run_script(ctx, r"""
proc witness() [error] {
  let data = b"a\0\xffbcd"
  (data[1..3]) == (b"\0\xff")
  (data[..2]) == (b"a\0")
  (data[3..]) == (b"bcd")
  (data[..]) == (data)
  (data[-3..-1]) == (b"bc")
  (data[-99..99]) == (data)
  (data[4..1]) == (b"")
  (data[99..]) == (b"")
  (data[..-99]) == (b"")
  (b""[-1..99]) == (b"")
  (data[1..5][1..-1]) == (b"\xffb")
  (b"  é🦀  ".trim()[..2]) == (b"\xc3\xa9")
  ("  é🦀  ".trim()[..1][..]) == ("é")
  (data[..3]) == (data.slice(0, length: 3))
  (data[3..]) == (data.slice(3, length: data.len() - 3))
}
witness()
""")?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  output.stdout == ""
}

test test_slice_list_aliases_keep_value_semantics [error] {
  var data = [1, 2, 3]
  var selected = data[1..]
  selected += [9]
  (data) == ([1, 2, 3])
  data += [8]
  (selected) == ([2, 3, 9])
}

test test_slice_evaluates_receiver_and_bounds_once_in_order [error] { |ctx|
  let output = test.run_script(ctx, r"""
proc receiver() [io] -> Bytes {
  print "receiver"
  return b"abcd"
}
proc bound(label: Str, value: Int) [io] -> Int {
  print $label
  return value
}
let part = receiver()[bound("start", 1)..bound("end", 3)]
print ${part.base64()}
let suffix = receiver()[bound("suffix", 2)..]
print ${suffix.base64()}
let all = receiver()[..]
print ${all.base64()}
""")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  (output.stdout) == ("receiver\nstart\nend\nYmM=\nreceiver\nsuffix\nY2Q=\nreceiver\nYWJjZA==\n")
}

test test_slice_brackets_preserve_distinct_offset_count_errors [error] { |ctx|
  let negative_offset = test.run_script(ctx, "let part = b\"abc\".slice(-1)\n")?
  (negative_offset.success) == (false)
  ("bytes-slice" in negative_offset.stderr)
  let past_end = test.run_script(ctx, "let part = b\"abc\".slice(4)\n")?
  (past_end.success) == (false)
  ("bytes-slice" in past_end.stderr)
  let negative_length = test.run_script(ctx, "let part = b\"abc\".slice(0, -1)\n")?
  (negative_length.success) == (false)
  ("bytes-slice" in negative_length.stderr)
  (b"abc"[-1..]) == (b"c")
  (b"abc"[4..]) == (b"")
  (b"abc"[..-1]) == (b"ab")
  let equivalent = test.run_script(ctx, r"""b"abc"[..9223372036854775807] == b"abc".slice(0, 9223372036854775807)
""")?
  let {success: equivalent_success, stderr: equivalent_message, ..} = equivalent
  assert equivalent_success, equivalent_message
}

test test_slice_dynamic_bounds_require_validation_and_receivers_keep_runtime_errors [error] { |ctx|
  let bound_error = test.run_script(ctx, "let bound: Any = true\nlet part = [1, 2][bound..]\n")?
  (bound_error.success) == (false)
  ("check.dynamic-boundary" in bound_error.stderr)
  let text_error = test.run_script(ctx, "let bound: Any = false\nlet part = \"é\"[..bound]\n")?
  (text_error.success) == (false)
  ("check.dynamic-boundary" in text_error.stderr)
  let start: Any = 1
  let end: Any = 3
  ([0, 1, 2, 3][(start.require(Int)?)..(end.require(Int)?)]) == ([1, 2])
  let invalid: Any = true
  test.error_kind(invalid.require(Int), "schema")?
  let receiver_error = test.run_script(ctx, "let receiver: Any = 42\nlet part = receiver[..]\n")?
  (receiver_error.success) == (false)
  ("cannot slice Int" in receiver_error.stderr)
}
