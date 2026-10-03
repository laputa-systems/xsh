test test_slice_existing_list_and_unicode_scalar_normalization {
  let values = [10, 20, 30, 40]
  assert values[1..3] == [20, 30]
  assert values[..2] == [10, 20]
  assert values[2..] == [30, 40]
  assert values[..] == values
  assert values[-3..-1] == [20, 30]
  assert values[-99..99] == values
  assert values[3..1] == []
  assert values[99..] == []
  assert values[..-99] == []
  assert values[2..2] == []
  assert [1][..0] == []
  let empty: List[Int] = []
  assert empty[-1..99] == empty
  let text = "aé🦀éz"
  assert text[1..3] == "é🦀"
  assert text[-3..-1] == "é"
  assert text[..] == text
  assert text[-99..99] == text
  assert text[3..1] == ""
  assert text[99..] == ""
  assert text[..-99] == ""
  assert ""[-1..99] == ""
}

test test_slice_bytes_normalization_and_nested_views { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc witness() [error] {
  let data = b"a\0\xffbcd"
  assert data[1..3] == b"\0\xff"
  assert data[..2] == b"a\0"
  assert data[3..] == b"bcd"
  assert data[..] == data
  assert data[-3..-1] == b"bc"
  assert data[-99..99] == data
  assert data[4..1] == b""
  assert data[99..] == b""
  assert data[..-99] == b""
  assert b""[-1..99] == b""
  assert data[1..5][1..-1] == b"\xffb"
  assert b"  é🦀  ".trim()[..2] == b"\xc3\xa9"
  assert "  é🦀  ".trim()[..1][..] == "é"
  assert data[..3] == data.slice(0, length: 3)
  assert data[3..] == data.slice(3, length: data.len() - 3)
}
witness()
""",
  )?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  assert output.stdout == ""
}

test test_slice_list_aliases_keep_value_semantics {
  var data = [1, 2, 3]
  var selected = data[1..]
  selected += [9]
  assert data == [1, 2, 3]
  data += [8]
  assert selected == [2, 3, 9]
}

test test_slice_evaluates_receiver_and_bounds_once_in_order { |ctx|
  let output = test.run_script(
    ctx,
    r"""
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
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """receiver
start
end
YmM=
receiver
suffix
Y2Q=
receiver
YWJjZA==
"""
}

test test_slice_brackets_preserve_distinct_offset_count_errors { |ctx|
  let negative_offset = test.run_script(
    ctx,
    """let part = b"abc".slice(-1)
""",
  )?
  assert negative_offset.success == false
  assert "bytes-slice" in negative_offset.stderr
  let past_end = test.run_script(
    ctx,
    """let part = b"abc".slice(4)
""",
  )?
  assert past_end.success == false
  assert "bytes-slice" in past_end.stderr
  let negative_length = test.run_script(
    ctx,
    """let part = b"abc".slice(0, -1)
""",
  )?
  assert negative_length.success == false
  assert "bytes-slice" in negative_length.stderr
  assert b"abc"[-1..] == b"c"
  assert b"abc"[4..] == b""
  assert b"abc"[..-1] == b"ab"
  let equivalent = test.run_script(
    ctx,
    r"""assert b"abc"[..9223372036854775807] == b"abc".slice(0, 9223372036854775807)
""",
  )?
  let {success: equivalent_success, stderr: equivalent_message, ..} = equivalent
  assert equivalent_success, equivalent_message
}

test test_slice_dynamic_bounds_require_validation_and_receivers_keep_runtime_errors { |ctx|
  let bound_error = test.run_script(
    ctx,
    """let bound: Any = true
let part = [1, 2][bound..]
""",
  )?
  assert bound_error.success == false
  assert "check.dynamic-boundary" in bound_error.stderr
  let text_error = test.run_script(
    ctx,
    """let bound: Any = false
let part = "é"[..bound]
""",
  )?
  assert text_error.success == false
  assert "check.dynamic-boundary" in text_error.stderr
  let start: Any = 1
  let end: Any = 3
  assert [0, 1, 2, 3][(start.require(Int)?)..end.require(Int)?] == [1, 2]
  let invalid: Any = true
  test.error_kind(invalid.require(Int), "schema")?
  let receiver_error = test.run_script(
    ctx,
    """let receiver: Any = 42
let part = receiver[..]
""",
  )?
  assert receiver_error.success == false
  assert "cannot slice Int" in receiver_error.stderr
}

test test_slice_rejects_colon_inclusive_stride_and_range_values { |ctx|
  for source in [
    """let part = [1, 2, 3][0:2]
""",
    """let part = [1, 2, 3][0..=2]
""",
    """let part = [1, 2, 3][0..3..1]
""",
    """let range = 0..2
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, output.stderr
    assert "parse." in output.stderr
  }
}
