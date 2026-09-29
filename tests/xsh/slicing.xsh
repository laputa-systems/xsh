proc test_slice_existing_list_and_unicode_scalar_normalization() [error] {
  let values = [10, 20, 30, 40]
  test.eq(values[1..3], [20, 30])?
  test.eq(values[..2], [10, 20])?
  test.eq(values[2..], [30, 40])?
  test.eq(values[..], values)?
  test.eq(values[-3..-1], [20, 30])?
  test.eq(values[-99..99], values)?
  test.eq(values[3..1], [])?
  test.eq(values[99..], [])?
  test.eq(values[..-99], [])?
  test.eq(values[2..2], [])?
  test.eq([1][..0], [])?
  let empty: List[Int] = []
  test.eq(empty[-1..99], empty)?
  let text = "aé🦀e\u{301}z"
  test.eq(text[1..3], "é🦀")?
  test.eq(text[-3..-1], "e\u{301}")?
  test.eq(text[..], text)?
  test.eq(text[-99..99], text)?
  test.eq(text[3..1], "")?
  test.eq(text[99..], "")?
  test.eq(text[..-99], "")?
  test.eq(""[-1..99], "")?
}

proc test_slice_bytes_normalization_and_nested_views() [error] {
  let data = b"a\0\xffbcd"
  test.eq(data[1..3], b"\0\xff")?
  test.eq(data[..2], b"a\0")?
  test.eq(data[3..], b"bcd")?
  test.eq(data[..], data)?
  test.eq(data[-3..-1], b"bc")?
  test.eq(data[-99..99], data)?
  test.eq(data[4..1], b"")?
  test.eq(data[99..], b"")?
  test.eq(data[..-99], b"")?
  test.eq(b""[-1..99], b"")?
  test.eq(data[1..5][1..-1], b"\xffb")?
  test.eq(b"  é🦀  ".trim()[..2], b"\xc3\xa9")?
  test.eq("  é🦀  ".trim()[..1][..], "é")?
  test.eq(data[..3], data.slice(0, length: 3))?
  test.eq(data[3..], data.slice(3, length: data.len() - 3))?
}

proc test_slice_list_aliases_keep_value_semantics() [error] {
  var data = [1, 2, 3]
  var selected = data[1..]
  selected = selected.push(9)
  test.eq(data, [1, 2, 3])?
  data = data.push(8)
  test.eq(selected, [2, 3, 9])?
}

proc test_slice_evaluates_receiver_and_bounds_once_in_order(ctx: TestContext) [error] {
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
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "receiver\nstart\nend\nYmM=\nreceiver\nsuffix\nY2Q=\nreceiver\nYWJjZA==\n")?
}

proc test_slice_brackets_preserve_distinct_offset_count_errors(ctx: TestContext) [error] {
  let negative_offset = test.run_script(ctx, "let part = b\"abc\".slice(-1)\n")?
  test.eq(negative_offset.success, false)?
  test.contains(negative_offset.stderr, "bytes-slice")?
  let past_end = test.run_script(ctx, "let part = b\"abc\".slice(4)\n")?
  test.eq(past_end.success, false)?
  test.contains(past_end.stderr, "bytes-slice")?
  let negative_length = test.run_script(ctx, "let part = b\"abc\".slice(0, -1)\n")?
  test.eq(negative_length.success, false)?
  test.contains(negative_length.stderr, "bytes-slice")?
  test.eq(b"abc"[-1..], b"c")?
  test.eq(b"abc"[4..], b"")?
  test.eq(b"abc"[..-1], b"ab")?
  test.eq(b"abc"[..9223372036854775807], b"abc".slice(0, 9223372036854775807))?
}

proc test_slice_dynamic_bounds_and_receivers_keep_runtime_type_errors(ctx: TestContext) [error] {
  let bound_error = test.run_script(ctx, "let bound: Any = true\nlet part = [1, 2][bound..]\n")?
  test.eq(bound_error.success, false)?
  test.contains(bound_error.stderr, "slice index expected Int")?
  let text_error = test.run_script(ctx, "let bound: Any = false\nlet part = \"é\"[..bound]\n")?
  test.eq(text_error.success, false)?
  test.contains(text_error.stderr, "slice index expected Int")?
  let receiver_error = test.run_script(ctx, "let receiver: Any = 42\nlet part = receiver[..]\n")?
  test.eq(receiver_error.success, false)?
  test.contains(receiver_error.stderr, "cannot slice Int")?
}
