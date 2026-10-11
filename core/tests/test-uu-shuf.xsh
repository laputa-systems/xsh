##! Transcribed random sampling tests from the uutils coreutils suite.

use support.uu as uu

proc numbers(data: Bytes, separator: Str = "\n") [error] -> Result[List[Int], Error] {
  var result: List[Int] = []
  for word in data.utf8()?.split(separator) {
    if word != "" { assert rx"^[+-]?[0-9]+$".matches(word); result += [word.parse_int()?] }
  }
  Ok(result)
}

proc sorted_numbers(data: Bytes, separator: Str = "\n") [error] -> Result[List[Int], Error] {
  Ok([value for value in numbers(data, separator)? |> sort-by .])
}

proc words(data: Bytes, separator: Str = "\n") [error] -> Result[List[Str], Error] {
  var result: List[Str] = []
  for word in data.utf8()?.split(separator) { if word != "" { result += [word] } }
  Ok(result)
}

# The unsigned 64-bit output domain exceeds Int; validate its decimal spelling
# without narrowing a sampled value into a signed integer.
pure unsigned64(word: Str) -> Bool {
  return false unless rx"^[+]?[0-9]+$".matches(word)
  var digits = if word.starts_with("+") { word.byte_slice(1) } else { word }
  while digits.byte_len() > 1 and digits.starts_with("0") { digits = digits.byte_slice(1) }
  digits.byte_len() < 20 or (digits.byte_len() == 20 and digits <= "18446744073709551615")
}

# origin: uutils test_shuf::test_output_is_random_permutation
test test_uu_shuf_output_is_random_permutation { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", [], stdin: bytes.from_text("1\n2\n3\n4\n5\n6\n7\n8\n9\n10"))?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert sorted_numbers(r.stdout, "\n")? == [value for value in range(1, 11)]
  assert r.stdout.utf8()? != "1\n2\n3\n4\n5\n6\n7\n8\n9\n10"
}

# origin: uutils test_shuf::test_explicit_stdin_file
test test_uu_shuf_explicit_stdin_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-"], stdin: bytes.from_text("1\n2\n3\n4\n5\n6\n7\n8\n9\n10"))?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert sorted_numbers(r.stdout, "\n")? == [value for value in range(1, 11)]
}

# origin: uutils test_shuf::test_zero_termination
test test_uu_shuf_zero_termination { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-z", "-i1-10"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert sorted_numbers(r.stdout, "\u{0}")? == [value for value in range(1, 11)]
}

# origin: uutils test_shuf::test_zero_termination_multi
test test_uu_shuf_zero_termination_multi { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-z", "-z", "-i1-10"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert sorted_numbers(r.stdout, "\u{0}")? == [value for value in range(1, 11)]
}

# origin: uutils test_shuf::test_echo
test test_uu_shuf_echo { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-e", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert sorted_numbers(r.stdout, "\n")? == [value for value in range(1, 11)]
}

# origin: uutils test_shuf::test_very_large_range
test test_uu_shuf_very_large_range { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n", "256", "-i1-100000000000"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let result = numbers(r.stdout)?
  assert result.len() == 256
  for number in result { assert number >= 0 and number <= 100000000000 }
}

# origin: uutils test_shuf::test_very_large_range_offset
test test_uu_shuf_very_large_range_offset { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n", "10", "-i1234567890-2147483647"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let result = numbers(r.stdout)?
  assert result.len() == 10
  for number in result { assert number >= 1234567890 and number <= 2147483647 }
}

# origin: uutils test_shuf::test_range_repeat
test test_uu_shuf_range_repeat { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-r", "-n", "500", "-i12-34"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let result = numbers(r.stdout)?
  assert result.len() == 500
  for number in result { assert number >= 12 and number <= 34 }
}

# origin: uutils test_shuf::test_range_repeat_no_overflow_1_max
test test_uu_shuf_range_repeat_no_overflow_1_max { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-rn1", "-i1-18446744073709551615"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let result = words(r.stdout)?
  assert result.len() == 1
  for word in result { assert unsigned64(word) }
}

# origin: uutils test_shuf::test_range_repeat_no_overflow_0_max_minus_1
test test_uu_shuf_range_repeat_no_overflow_0_max_minus_1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-rn1", "-i0-18446744073709551614"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let result = words(r.stdout)?
  assert result.len() == 1
  for word in result { assert unsigned64(word) }
}

# origin: uutils test_shuf::test_range_permute_no_overflow_1_max
test test_uu_shuf_range_permute_no_overflow_1_max { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n1", "-i1-18446744073709551615"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let result = words(r.stdout)?
  assert result.len() == 1
  for word in result { assert unsigned64(word) }
}

# origin: uutils test_shuf::test_range_permute_no_overflow_0_max_minus_1
test test_uu_shuf_range_permute_no_overflow_0_max_minus_1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n1", "-i0-18446744073709551614"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  let result = words(r.stdout)?
  assert result.len() == 1
  for word in result { assert unsigned64(word) }
}

# origin: uutils test_shuf::test_range_full_huge_no_head_count_memory_exhausted
test test_uu_shuf_range_full_huge_no_head_count_memory_exhausted { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-i1-18446744073709551615"], timeout: 10s)?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "shuf: memory exhausted\n")
}

# origin: uutils test_shuf::test_range_huge_head_count_memory_exhausted
test test_uu_shuf_range_huge_head_count_memory_exhausted { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n18446744073709551615", "-i1-18446744073709551615"], timeout: 10s)?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "shuf: memory exhausted\n")
}

# origin: uutils test_shuf::test_very_high_range_full
test test_uu_shuf_very_high_range_full { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-i2147483641-2147483647"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert sorted_numbers(r.stdout)? == [value for value in range(2147483641, 2147483648)]
}

# origin: uutils test_shuf::test_echo_multi
test test_uu_shuf_echo_multi { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-e", "a", "b", "-e", "c"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert [word for word in words(r.stdout, "\n")? |> sort-by .] == ["a", "b", "c"]
}

# origin: uutils test_shuf::test_echo_postfix
test test_uu_shuf_echo_postfix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["a", "b", "c", "-e"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert [word for word in words(r.stdout, "\n")? |> sort-by .] == ["a", "b", "c"]
}

# origin: uutils test_shuf::test_echo_short_collapsed_zero
test test_uu_shuf_echo_short_collapsed_zero { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-ez", "a", "b", "c"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert [word for word in words(r.stdout, "\u{0}")? |> sort-by .] == ["a", "b", "c"]
}

# origin: uutils test_shuf::test_echo_separators_in_arguments
test test_uu_shuf_echo_separators_in_arguments { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-e", "-n2", "a\nb", "c\nd"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.len() == 8
}

# origin: uutils test_shuf::test_echo_invalid_unicode_in_arguments
test test_uu_shuf_echo_invalid_unicode_in_arguments { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_paths(s, "shuf", [p"-e", Path.parse_bytes(b"a\xffb")?, p"ok"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert 255 in [r.stdout.byte_at(index) for index in range(r.stdout.len())]
}

# origin: uutils test_shuf::test_invalid_unicode_in_filename
test test_uu_shuf_invalid_unicode_in_filename { |ctx|
  let s = uu.scene(ctx)?
  let name = uu.at_bytes(s, b"a\xffb")?
  name.write("foo\n")?
  let r = uu.invoke_paths(s, "shuf", [Path.parse_bytes(b"a\xffb")?])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, b"foo\n")
}

# origin: uutils test_shuf::test_head_count
test test_uu_shuf_head_count { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n", "5"], stdin: bytes.from_text("1\n2\n3\n4\n5\n6\n7\n8\n9\n10"))?
  uu.succeeds(r)
  uu.no_stderr(r)
  let result = sorted_numbers(r.stdout)?
  assert result.len() == 5
  for number in result { assert number in [value for value in range(1, 11)] }
}

# origin: uutils test_shuf::test_head_count_multi_big_then_small
test test_uu_shuf_head_count_multi_big_then_small { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n", "6", "-n", "5"], stdin: bytes.from_text("1\n2\n3\n4\n5\n6\n7\n8\n9\n10"))?
  uu.succeeds(r)
  uu.no_stderr(r)
  let result = numbers(r.stdout)?
  assert result.len() == 5
  for number in result { assert number in [value for value in range(1, 11)] }
}

# origin: uutils test_shuf::test_head_count_multi_small_then_big
test test_uu_shuf_head_count_multi_small_then_big { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n", "5", "-n", "6"], stdin: bytes.from_text("1\n2\n3\n4\n5\n6\n7\n8\n9\n10"))?
  uu.succeeds(r)
  uu.no_stderr(r)
  let result = numbers(r.stdout)?
  assert result.len() == 5
  for number in result { assert number in [value for value in range(1, 11)] }
}

# origin: uutils test_shuf::test_repeat
test test_uu_shuf_repeat { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-r", "-n", "15000"], stdin: bytes.from_text("1\n2\n3\n4\n5\n6\n7\n8\n9\n10"))?
  uu.succeeds(r)
  uu.no_stderr(r)
  let result = numbers(r.stdout)?
  assert result.len() == 15000
  for number in result { assert number in [value for value in range(1, 11)] }
}

# origin: uutils test_shuf::test_repeat_multi
test test_uu_shuf_repeat_multi { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-r", "-r", "-n", "15000"], stdin: bytes.from_text("1\n2\n3\n4\n5\n6\n7\n8\n9\n10"))?
  uu.succeeds(r)
  uu.no_stderr(r)
  let result = numbers(r.stdout)?
  assert result.len() == 15000
  for number in result { assert number in [value for value in range(1, 11)] }
}

# origin: uutils test_shuf::test_invalid_arg
test test_uu_shuf_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_shuf::test_empty_input
test test_uu_shuf_empty_input { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", [])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.no_stdout(r)
}

# origin: uutils test_shuf::test_zero_head_count_pipe
test test_uu_shuf_zero_head_count_pipe { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n0"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_shuf::test_zero_head_count_pipe_explicit
test test_uu_shuf_zero_head_count_pipe_explicit { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n0", "-"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_shuf::test_zero_head_count_file_unreadable
test test_uu_shuf_zero_head_count_file_unreadable { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n0", "/invalid/unreadable"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_shuf::test_zero_head_count_file_touch_output_negative
test test_uu_shuf_zero_head_count_file_touch_output_negative { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n0", "-o", "/invalid/unwritable"])?
  uu.fails(r)
  uu.stderr_contains(r, "shuf: /invalid/unwritable: No such file or directory")
}

# origin: uutils test_shuf::test_zero_head_count_echo
test test_uu_shuf_zero_head_count_echo { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n0", "-e", "hello"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_shuf::test_zero_head_count_range
test test_uu_shuf_zero_head_count_range { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n0", "-i4-8"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_shuf::test_shuf_echo_and_input_range_not_allowed
test test_uu_shuf_shuf_echo_and_input_range_not_allowed { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-e", "0", "-i", "0-2"])?
  uu.fails(r)
  uu.stderr_contains(r, "shuf: cannot combine -e and -i options")
}

# origin: uutils test_shuf::test_shuf_input_range_and_file_not_allowed
test test_uu_shuf_shuf_input_range_and_file_not_allowed { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-i", "0-9", "file"])?
  uu.fails(r)
  uu.stderr_contains(r, "shuf: extra operand 'file'")
}

# origin: uutils test_shuf::test_shuf_invalid_input_range_one
test test_uu_shuf_shuf_invalid_input_range_one { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-i", "0"])?
  uu.fails(r)
  uu.stderr_contains(r, "shuf: invalid input range: '0'")
}

# origin: uutils test_shuf::test_shuf_invalid_input_range_two
test test_uu_shuf_shuf_invalid_input_range_two { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-i", "a-9"])?
  uu.fails(r)
  uu.stderr_contains(r, "shuf: invalid input range: 'a-9'")
}

# origin: uutils test_shuf::test_shuf_invalid_input_range_three
test test_uu_shuf_shuf_invalid_input_range_three { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-i", "0-b"])?
  uu.fails(r)
  uu.stderr_contains(r, "shuf: invalid input range: '0-b'")
}

# origin: uutils test_shuf::test_shuf_multiple_input_ranges
test test_uu_shuf_shuf_multiple_input_ranges { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-i", "2-9", "-i", "2-9"])?
  uu.fails(r)
  uu.stderr_contains(r, "multiple -i")
  uu.stderr_contains(r, "options specified")
}

# origin: uutils test_shuf::test_shuf_multiple_outputs
test test_uu_shuf_shuf_multiple_outputs { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-o", "file_a", "-o", "file_b"])?
  uu.fails(r)
  uu.stderr_contains(r, "multiple output files")
  uu.stderr_contains(r, "files specified")
}

# origin: uutils test_shuf::test_shuf_two_input_files
test test_uu_shuf_shuf_two_input_files { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["file_a", "file_b"])?
  uu.fails(r)
  uu.stderr_contains(r, "shuf: extra operand 'file_b'")
}

# origin: uutils test_shuf::test_shuf_three_input_files
test test_uu_shuf_shuf_three_input_files { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["file_a", "file_b", "file_c"])?
  uu.fails(r)
  uu.stderr_contains(r, "shuf: extra operand 'file_b'")
}

# origin: uutils test_shuf::test_shuf_invalid_input_line_count
test test_uu_shuf_shuf_invalid_input_line_count { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n", "a"])?
  uu.fails(r)
  uu.stderr_contains(r, "shuf: invalid line count: 'a'")
}

# origin: uutils test_shuf::test_shuf_repeat_empty_range
test test_uu_shuf_shuf_repeat_empty_range { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-ri4-3"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_only(r, "shuf: no lines to repeat\n")
}

# origin: uutils test_shuf::test_shuf_repeat_empty_echo
test test_uu_shuf_shuf_repeat_empty_echo { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-re"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_only(r, "shuf: no lines to repeat\n")
}

# origin: uutils test_shuf::test_shuf_repeat_empty_input
test test_uu_shuf_shuf_repeat_empty_input { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-r"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_only(r, "shuf: no lines to repeat\n")
}

# origin: uutils test_shuf::test_range_one_elem
test test_uu_shuf_range_one_elem { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-i5-5"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_only(r, "5\n")
}

# origin: uutils test_shuf::test_range_empty
test test_uu_shuf_range_empty { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-i5-4"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_shuf::test_range_empty_minus_one
test test_uu_shuf_range_empty_minus_one { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-i5-3"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "shuf: invalid input range: '5-3'")
}

# origin: uutils test_shuf::test_range_repeat_one_elem
test test_uu_shuf_range_repeat_one_elem { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n1", "-ri5-5"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_only(r, "5\n")
}

# origin: uutils test_shuf::test_range_repeat_empty
test test_uu_shuf_range_repeat_empty { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n1", "-ri5-4"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_only(r, "shuf: no lines to repeat\n")
}

# origin: uutils test_shuf::test_range_repeat_empty_minus_one
test test_uu_shuf_range_repeat_empty_minus_one { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n1", "-ri5-3"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "shuf: invalid input range: '5-3'")
}

# origin: uutils test_shuf::test_empty_range_no_repeat
test test_uu_shuf_empty_range_no_repeat { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-i4-3"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_shuf::test_zero_head_count_file_touch_output_positive_new
test test_uu_shuf_zero_head_count_file_touch_output_positive_new { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n0", "-o", "file"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.read(s, "file")? == b""
}

# origin: uutils test_shuf::test_zero_head_count_file_touch_output_positive_existing
test test_uu_shuf_zero_head_count_file_touch_output_positive_existing { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  let r = uu.invoke(s, "shuf", ["-n0", "-o", "file"])?
  uu.succeeds(r)
  uu.no_output(r)
  assert uu.read(s, "file")? == b""
}

# origin: uutils test_shuf::test_output_not_truncated_when_input_missing
test test_uu_shuf_output_not_truncated_when_input_missing { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "out", "keep me\n")?
  let r = uu.invoke(s, "shuf", ["-o", "out", "does-not-exist"])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "does-not-exist")
  uu.file_is(s, "out", "keep me\n")
}

# origin: uutils test_shuf::test_output_not_truncated_when_random_source_missing
test test_uu_shuf_output_not_truncated_when_random_source_missing { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "out", "keep me\n")?
  uu.write(s, "in", "a\nb\nc\n")?
  let r = uu.invoke(s, "shuf", ["-o", "out", "--random-source=does-not-exist", "in"])?
  uu.fails_with_code(r, 1)
  uu.file_is(s, "out", "keep me\n")
}

# origin: uutils test_shuf::test_file_input
test test_uu_shuf_file_input { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "shuf", "file_input.txt", "file_input.txt")?
  let r = uu.invoke(s, "shuf", ["file_input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert sorted_numbers(r.stdout)? == [value for value in range(11, 21)]
}

# origin: uutils test_shuf::test_shuf_multiple_input_line_count
test test_uu_shuf_shuf_multiple_input_line_count { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-i10-200", "-n", "10", "-n", "5"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert words(r.stdout)?.len() == 5
}

# origin: uutils test_shuf::test_head_count_does_not_overflow_file
test test_uu_shuf_head_count_does_not_overflow_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input.txt", "hello\n")?
  let r = uu.invoke(s, "shuf", ["-n4294967296", "input.txt"])?
  uu.succeeds(r)
  uu.stdout_is(r, "hello\n")
  uu.no_stderr(r)
}

# origin: uutils test_shuf::test_head_count_does_not_overflow_args
test test_uu_shuf_head_count_does_not_overflow_args { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n4294967296", "-e", "goodbye"])?
  uu.succeeds(r)
  uu.stdout_is(r, "goodbye\n")
  uu.no_stderr(r)
}

# origin: uutils test_shuf::test_head_count_does_not_overflow_range
test test_uu_shuf_head_count_does_not_overflow_range { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "shuf", ["-n4294967296", "-i1-1"])?
  uu.succeeds(r)
  uu.stdout_is(r, "1\n")
  uu.no_stderr(r)
}

# origin: uutils test_shuf::test_gnu_compat_args_no_repeat
test test_uu_shuf_gnu_compat_args_no_repeat { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "random_bytes.bin", b"\xd1\xfd\xb9\x9a\xf5\x81\x71\x42\xf9\x7a\x59\x79\xd4\x9c\x8c\x7d")?
  let r = uu.invoke(s, "shuf", ["--random-source=random_bytes.bin", "-e", "1", "2", "3", "4", "5", "6", "7"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "7\n1\n2\n5\n3\n4\n6\n")
}

# origin: uutils test_shuf::test_gnu_compat_from_stdin
test test_uu_shuf_gnu_compat_from_stdin { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "random_bytes.bin", b"\xd1\xfd\xb9\x9a\xf5\x81\x71\x42\xf9\x7a\x59\x79\xd4\x9c\x8c\x7d")?
  uu.write(s, "input.txt", "1\n2\n3\n4\n5\n6\n7\n")?
  let r = uu.invoke_from_path(s, "shuf", ["--random-source=random_bytes.bin"], stdin: uu.at(s, "input.txt"))?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "7\n1\n2\n5\n3\n4\n6\n")
}

# origin: uutils test_shuf::test_gnu_compat_from_file
test test_uu_shuf_gnu_compat_from_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "random_bytes.bin", b"\xd1\xfd\xb9\x9a\xf5\x81\x71\x42\xf9\x7a\x59\x79\xd4\x9c\x8c\x7d")?
  uu.write(s, "input.txt", "1\n2\n3\n4\n5\n6\n7\n")?
  let r = uu.invoke(s, "shuf", ["--random-source=random_bytes.bin", "input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "7\n1\n2\n5\n3\n4\n6\n")
}

# origin: uutils test_shuf::test_gnu_compat_limited_from_file
test test_uu_shuf_gnu_compat_limited_from_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "random_bytes.bin", b"\xd1\xfd\xb9\x9a\xf5\x81\x71\x42\xf9\x7a\x59\x79\xd4\x9c\x8c\x7d")?
  uu.write(s, "input.txt", "1\n2\n3\n4\n5\n6\n7\n")?
  let r = uu.invoke(s, "shuf", ["--random-source=random_bytes.bin", "-n5", "input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "7\n1\n2\n5\n3\n")
}

# origin: uutils test_shuf::test_gnu_compat_range_no_repeat
test test_uu_shuf_gnu_compat_range_no_repeat { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "random_bytes.bin", b"\xd1\xfd\xb9\x9a\xf5\x81\x71\x42\xf9\x7a\x59\x79\xd4\x9c\x8c\x7d")?
  let r = uu.invoke(s, "shuf", ["--random-source=random_bytes.bin", "-i1-10"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "10\n2\n8\n7\n3\n9\n6\n5\n1\n4\n")
}

# origin: uutils test_shuf::test_gnu_compat_range_repeat
test test_uu_shuf_gnu_compat_range_repeat { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "random_bytes.bin", b"\xfb\x83\x8f\x21\x9b\x3c\x2d\xc5\x73\xa5\x58\x6c\x54\x2f\x59\xf8")?
  let r = uu.invoke(s, "shuf", ["--random-source=random_bytes.bin", "-r", "-i1-99"], timeout: 10s)?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "shuf: 'random_bytes.bin': end of file\n")
  uu.stdout_is(r, "38\n30\n10\n26\n23\n61\n46\n99\n75\n43\n10\n89\n10\n44\n24\n59\n22\n51\n")
}

# origin: uutils test_shuf::test_getrandom_fail
test test_uu_shuf_getrandom_fail { |ctx|
  let s = uu.scene(ctx)?
  let tracer = match process.which("strace") { Ok(executable) => executable, Err(_) => { test.skip("strace is unavailable"); return } }
  let launch_words = uu.argv(s, "shuf", [p"-i", p"1234-1235"])?
  let argv = [tracer, p"-o", p"/dev/null", p"-e", p"inject=getrandom:error=EAGAIN"].extend(launch_words)
  let out = uu.at(s, "trace-out")
  let err = uu.at(s, "trace-err")
  let status = process.run(process.command_argv(tracer, argv, s.root, {LC_ALL: "C", TZ: "UTC"}, b"", out, err, timeout: 10s))?
  assert status.exited_with(0), err.read_text()?
  assert "1234" in out.read_text()?
}
