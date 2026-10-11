##! Transcribed from the uutils csplit integration tests.

use support.uu as uu

pure generate(from: Int, to: Int) -> Str {
  [f"{value}\n" for value in range(from, to)].join("")
}

# origin: uutils test_csplit::no_such_file
test test_uu_csplit_no_such_file { |ctx|
  let s1 = uu.scene(ctx)?
  let r1 = uu.invoke(s1, "csplit", ["in", "0"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_contains(r1, "cannot open 'in' for reading: No such file or directory")
}

# origin: uutils test_csplit::precision_format
test test_uu_csplit_precision_format { |ctx|
  for f in ["%#6.3x", "%0#6.3x"] {
    let s1 = uu.scene(ctx)?
    uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
    let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "10", "--suffix-format", f], stdin: b"")?
    uu.succeeds(r1)
    uu.stdout_only(r1, "18\n123\n")
    let count1 = s1.root.glob("xx*")?.len()
    assert count1 == 2
    uu.file_is(s1, "xx   000", generate(1, 10))?
    uu.file_is(s1, "xx 0x001", generate(10, 51))?
  }
}

# origin: uutils test_csplit::suffix_format_hyphen_leading_as_separate_arg
test test_uu_csplit_suffix_format_hyphen_leading_as_separate_arg { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "10", "--suffix-format", "-%02d"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "18\n123\n")
  uu.file_is(s1, "xx-00", generate(1, 10))?
  uu.file_is(s1, "xx-01", generate(10, 51))?
}

# origin: uutils test_csplit::test_corner_case1
test test_uu_csplit_corner_case1 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/10/", "11"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "18\n3\n120\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 3
  uu.file_is(s1, "xx00", generate(1, 10))?
  uu.file_is(s1, "xx01", "10\n")?
  uu.file_is(s1, "xx02", generate(11, 51))?
}

# origin: uutils test_csplit::test_corner_case2
test test_uu_csplit_corner_case2 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/10/-5", "/10/"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_is(r1, "csplit: '/10/': match not found\n")
  uu.stdout_is(r1, "8\n133\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
}

# origin: uutils test_csplit::test_corner_case3
test test_uu_csplit_corner_case3 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/15/-3", "14", "/15/"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_is(r1, "csplit: '/15/': match not found\n")
  uu.stdout_is(r1, "24\n6\n111\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
}

# origin: uutils test_csplit::test_corner_case4
test test_uu_csplit_corner_case4 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/20/-10", "/30/-4"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "18\n48\n75\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 3
  uu.file_is(s1, "xx00", generate(1, 10))?
  uu.file_is(s1, "xx01", generate(10, 26))?
  uu.file_is(s1, "xx02", generate(26, 51))?
}

# origin: uutils test_csplit::test_empty_regex_matches_every_line
test test_uu_csplit_empty_regex_matches_every_line { |ctx|
  let s1 = uu.scene(ctx)?
  uu.write(s1, "letters", "delta\necho\nfoxtrot\n")?
  let r1 = uu.invoke(s1, "csplit", ["letters", "//"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0\n19\n")
  uu.file_is(s1, "xx00", "")?
  uu.file_is(s1, "xx01", "delta\necho\nfoxtrot\n")?
}

# origin: uutils test_csplit::test_empty_skip_to_regex_is_accepted
test test_uu_csplit_empty_skip_to_regex_is_accepted { |ctx|
  let s1 = uu.scene(ctx)?
  uu.write(s1, "letters", "delta\necho\nfoxtrot\n")?
  let r1 = uu.invoke(s1, "csplit", ["letters", "%%"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "19\n")
  uu.file_is(s1, "xx00", "delta\necho\nfoxtrot\n")?
}

# origin: uutils test_csplit::test_invalid_arg
test test_uu_csplit_invalid_arg { |ctx|
  let s1 = uu.scene(ctx)?
  let r1 = uu.invoke(s1, "csplit", ["--definitely-invalid"], stdin: b"")?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_csplit::test_line_num_out_of_range1
test test_uu_csplit_line_num_out_of_range1 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "100"], stdin: b"")?
  uu.fails(r1)
  uu.stdout_is(r1, "141\n")
  uu.stderr_is(r1, "csplit: '100': line number out of range\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
  let s2 = uu.scene(ctx)?
  uu.fixture(s2, "csplit", "numbers50.txt", "numbers50.txt")?
  let r2 = uu.invoke(s2, "csplit", ["numbers50.txt", "100", "-k"], stdin: b"")?
  uu.fails(r2)
  uu.stdout_is(r2, "141\n")
  uu.stderr_is(r2, "csplit: '100': line number out of range\n")
  let count2 = s2.root.glob("xx*")?.len()
  assert count2 == 1
  uu.file_is(s2, "xx00", generate(1, 51))?
}

# origin: uutils test_csplit::test_line_num_out_of_range2
test test_uu_csplit_line_num_out_of_range2 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "10", "100"], stdin: b"")?
  uu.fails(r1)
  uu.stdout_is(r1, "18\n123\n")
  uu.stderr_is(r1, "csplit: '100': line number out of range\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
  let s2 = uu.scene(ctx)?
  uu.fixture(s2, "csplit", "numbers50.txt", "numbers50.txt")?
  let r2 = uu.invoke(s2, "csplit", ["numbers50.txt", "10", "100", "-k"], stdin: b"")?
  uu.fails(r2)
  uu.stdout_is(r2, "18\n123\n")
  uu.stderr_is(r2, "csplit: '100': line number out of range\n")
  let count2 = s2.root.glob("xx*")?.len()
  assert count2 == 2
  uu.file_is(s2, "xx00", generate(1, 10))?
  uu.file_is(s2, "xx01", generate(10, 51))?
}

# origin: uutils test_csplit::test_line_num_out_of_range3
test test_uu_csplit_line_num_out_of_range3 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "40", "{2}"], stdin: b"")?
  uu.fails(r1)
  uu.stdout_is(r1, "108\n33\n")
  uu.stderr_is(r1, "csplit: '40': line number out of range on repetition 1\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
  let s2 = uu.scene(ctx)?
  uu.fixture(s2, "csplit", "numbers50.txt", "numbers50.txt")?
  let r2 = uu.invoke(s2, "csplit", ["numbers50.txt", "40", "{2}", "-k"], stdin: b"")?
  uu.fails(r2)
  uu.stdout_is(r2, "108\n33\n")
  uu.stderr_is(r2, "csplit: '40': line number out of range on repetition 1\n")
  let count2 = s2.root.glob("xx*")?.len()
  assert count2 == 2
  uu.file_is(s2, "xx00", generate(1, 40))?
  uu.file_is(s2, "xx01", generate(40, 51))?
}

# origin: uutils test_csplit::test_line_num_out_of_range4
test test_uu_csplit_line_num_out_of_range4 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "40", "{*}"], stdin: b"")?
  uu.fails(r1)
  uu.stdout_is(r1, "108\n33\n")
  uu.stderr_is(r1, "csplit: '40': line number out of range on repetition 1\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
  let s2 = uu.scene(ctx)?
  uu.fixture(s2, "csplit", "numbers50.txt", "numbers50.txt")?
  let r2 = uu.invoke(s2, "csplit", ["numbers50.txt", "40", "{*}", "-k"], stdin: b"")?
  uu.fails(r2)
  uu.stdout_is(r2, "108\n33\n")
  uu.stderr_is(r2, "csplit: '40': line number out of range on repetition 1\n")
  let count2 = s2.root.glob("xx*")?.len()
  assert count2 == 2
  uu.file_is(s2, "xx00", generate(1, 40))?
  uu.file_is(s2, "xx01", generate(40, 51))?
}

# origin: uutils test_csplit::test_line_num_range_with_up_to_match1
test test_uu_csplit_line_num_range_with_up_to_match1 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "10", "/12/-5"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_is(r1, "csplit: '/12/-5': line number out of range\n")
  uu.stdout_is(r1, "18\n0\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
  let s2 = uu.scene(ctx)?
  uu.fixture(s2, "csplit", "numbers50.txt", "numbers50.txt")?
  let r2 = uu.invoke(s2, "csplit", ["numbers50.txt", "10", "/12/-5", "-k"], stdin: b"")?
  uu.fails(r2)
  uu.stderr_is(r2, "csplit: '/12/-5': line number out of range\n")
  uu.stdout_is(r2, "18\n0\n")
  let count2 = s2.root.glob("xx*")?.len()
  assert count2 == 2
  uu.file_is(s2, "xx00", generate(1, 10))?
  uu.file_is(s2, "xx01", "")?
}

# origin: uutils test_csplit::test_line_num_range_with_up_to_match2
test test_uu_csplit_line_num_range_with_up_to_match2 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "10", "/12/-15"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_is(r1, "csplit: '/12/-15': line number out of range\n")
  uu.stdout_is(r1, "18\n0\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
  let s2 = uu.scene(ctx)?
  uu.fixture(s2, "csplit", "numbers50.txt", "numbers50.txt")?
  let r2 = uu.invoke(s2, "csplit", ["numbers50.txt", "10", "/12/-15", "-k"], stdin: b"")?
  uu.fails(r2)
  uu.stderr_is(r2, "csplit: '/12/-15': line number out of range\n")
  uu.stdout_is(r2, "18\n0\n")
  let count2 = s2.root.glob("xx*")?.len()
  assert count2 == 2
  uu.file_is(s2, "xx00", generate(1, 10))?
  uu.file_is(s2, "xx01", "")?
}

# origin: uutils test_csplit::test_line_numbers_suppress_matched_final_empty
test test_uu_csplit_line_numbers_suppress_matched_final_empty { |ctx|
  let s1 = uu.scene(ctx)?
  let r1 = uu.invoke(s1, "csplit", ["--suppress-matched", "-", "2", "4", "6"], stdin: bytes.from_text("1\n2\n3\n4\n5\n6\n"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "2\n2\n2\n0\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 4
  uu.file_is(s1, "xx00", "1\n")?
  uu.file_is(s1, "xx01", "3\n")?
  uu.file_is(s1, "xx02", "5\n")?
  uu.file_is(s1, "xx03", "")?
}

# origin: uutils test_csplit::test_line_numbers_suppress_matched_final_empty_elided_with_z
test test_uu_csplit_line_numbers_suppress_matched_final_empty_elided_with_z { |ctx|
  let s1 = uu.scene(ctx)?
  let r1 = uu.invoke(s1, "csplit", ["--suppress-matched", "-z", "-", "2", "4", "6"], stdin: bytes.from_text("1\n2\n3\n4\n5\n6\n"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "2\n2\n2\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 3
  uu.file_is(s1, "xx00", "1\n")?
  uu.file_is(s1, "xx01", "3\n")?
  uu.file_is(s1, "xx02", "5\n")?
}

# origin: uutils test_csplit::test_mix
test test_uu_csplit_mix { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "13", "%25%", "/0$/"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "27\n15\n63\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 3
  uu.file_is(s1, "xx00", generate(1, 13))?
  uu.file_is(s1, "xx01", generate(25, 30))?
  uu.file_is(s1, "xx02", generate(30, 51))?
}

# origin: uutils test_csplit::test_negative_offset_at_start
test test_uu_csplit_negative_offset_at_start { |ctx|
  let s1 = uu.scene(ctx)?
  let r1 = uu.invoke(s1, "csplit", ["-", "/a/-1", "{*}"], stdin: bytes.from_text("\na\n"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0\n3\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 2
  uu.file_is(s1, "xx00", "")?
  uu.file_is(s1, "xx01", "\na\n")?
}

# origin: uutils test_csplit::test_no_match
test test_uu_csplit_no_match { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%nope%"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_only(r1, "csplit: '%nope%': match not found\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
  let s2 = uu.scene(ctx)?
  uu.fixture(s2, "csplit", "numbers50.txt", "numbers50.txt")?
  let r2 = uu.invoke(s2, "csplit", ["numbers50.txt", "/nope/"], stdin: b"")?
  uu.fails(r2)
  uu.stdout_is(r2, "141\n")
  uu.stderr_is(r2, "csplit: '/nope/': match not found\n")
  let count2 = s2.root.glob("xx*")?.len()
  assert count2 == 0
}

# origin: uutils test_csplit::test_option_elide_empty_file1
test test_uu_csplit_option_elide_empty_file1 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "--suppress-matched", "-z", "/0$/", "{*}"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "18\n27\n27\n27\n27\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 5
  uu.file_is(s1, "xx00", generate(1, 10))?
  uu.file_is(s1, "xx01", generate(11, 20))?
  uu.file_is(s1, "xx02", generate(21, 30))?
  uu.file_is(s1, "xx03", generate(31, 40))?
  uu.file_is(s1, "xx04", generate(41, 50))?
}

# origin: uutils test_csplit::test_option_elide_empty_file2
test test_uu_csplit_option_elide_empty_file2 { |ctx|
  let s1 = uu.scene(ctx)?
  let r1 = uu.invoke(s1, "csplit", ["-", "-z", "/a/-1", "{*}"], stdin: bytes.from_text("\na\n"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "3\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 1
  uu.file_is(s1, "xx00", "\na\n")?
}

# origin: uutils test_csplit::test_option_keep
test test_uu_csplit_option_keep { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["-k", "numbers50.txt", "/20/", "/nope/"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_is(r1, "csplit: '/nope/': match not found\n")
  uu.stdout_is(r1, "48\n93\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 2
  uu.file_is(s1, "xx00", generate(1, 20))?
  uu.file_is(s1, "xx01", generate(20, 51))?
}

# origin: uutils test_csplit::test_option_prefix
test test_uu_csplit_option_prefix { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["--prefix", "dog", "numbers50.txt", "13", "%25%", "/0$/"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "27\n15\n63\n")
  let count1 = s1.root.glob("dog*")?.len()
  assert count1 == 3
  uu.file_is(s1, "dog00", generate(1, 13))?
  uu.file_is(s1, "dog01", generate(25, 30))?
  uu.file_is(s1, "dog02", generate(30, 51))?
}

# origin: uutils test_csplit::test_option_quiet
test test_uu_csplit_option_quiet { |ctx|
  for arg in ["-q", "--quiet", "-s", "--silent"] {
    let s1 = uu.scene(ctx)?
    uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
    let r1 = uu.invoke(s1, "csplit", [arg, "numbers50.txt", "13", "%25%", "/0$/"], stdin: b"")?
    uu.succeeds(r1)
    uu.no_stdout(r1)
    let count1 = s1.root.glob("xx*")?.len()
    assert count1 == 3
    uu.file_is(s1, "xx00", generate(1, 13))?
    uu.file_is(s1, "xx01", generate(25, 30))?
    uu.file_is(s1, "xx02", generate(30, 51))?
    uu.remove(s1, "xx00")?
    uu.remove(s1, "xx01")?
    uu.remove(s1, "xx02")?
  }
}

# origin: uutils test_csplit::test_skip_to_match
test test_uu_csplit_skip_to_match { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%23%"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "84\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 1
  uu.file_is(s1, "xx00", generate(23, 51))?
}

# origin: uutils test_csplit::test_skip_to_match_context_overflow
test test_uu_csplit_skip_to_match_context_overflow { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%45%+10"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_is(r1, "csplit: '%45%+10': line number out of range\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
  let s2 = uu.scene(ctx)?
  uu.fixture(s2, "csplit", "numbers50.txt", "numbers50.txt")?
  let r2 = uu.invoke(s2, "csplit", ["numbers50.txt", "%45%+10", "-k"], stdin: b"")?
  uu.fails(r2)
  uu.stderr_only(r2, "csplit: '%45%+10': line number out of range\n")
  let count2 = s2.root.glob("xx*")?.len()
  assert count2 == 0
}

# origin: uutils test_csplit::test_skip_to_match_context_underflow
test test_uu_csplit_skip_to_match_context_underflow { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%5%-10"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_only(r1, "csplit: '%5%-10': line number out of range\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
  let s2 = uu.scene(ctx)?
  uu.fixture(s2, "csplit", "numbers50.txt", "numbers50.txt")?
  let r2 = uu.invoke(s2, "csplit", ["numbers50.txt", "%5%-10", "-k"], stdin: b"")?
  uu.fails(r2)
  uu.stderr_only(r2, "csplit: '%5%-10': line number out of range\n")
  let count2 = s2.root.glob("xx*")?.len()
  assert count2 == 0
}

# origin: uutils test_csplit::test_skip_to_match_negative_offset
test test_uu_csplit_skip_to_match_negative_offset { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%23%-3"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "93\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 1
  uu.file_is(s1, "xx00", generate(20, 51))?
}

# origin: uutils test_csplit::test_skip_to_match_negative_offset_before_a_line_num
test test_uu_csplit_skip_to_match_negative_offset_before_a_line_num { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/20/-10", "15"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "18\n15\n108\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 3
  uu.file_is(s1, "xx00", generate(1, 10))?
  uu.file_is(s1, "xx01", generate(10, 15))?
  uu.file_is(s1, "xx02", generate(15, 51))?
}

# origin: uutils test_csplit::test_skip_to_match_negative_offset_before_a_match
test test_uu_csplit_skip_to_match_negative_offset_before_a_match { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/20/-10", "/15/"], stdin: b"")?
  uu.fails(r1)
  uu.stdout_is(r1, "18\n123\n")
  uu.stderr_is(r1, "csplit: '/15/': match not found\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
}

# origin: uutils test_csplit::test_skip_to_match_negative_offset_before_split_start
test test_uu_csplit_skip_to_match_negative_offset_before_split_start { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%3$%-3"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_is(r1, "csplit: '%3$%-3': line number out of range\n")
  assert s1.root.glob("xx*")?.len() == 0
}

# origin: uutils test_csplit::test_skip_to_match_negative_offset_min_i32
test test_uu_csplit_skip_to_match_negative_offset_min_i32 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%45%-2147483648"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_is(r1, "csplit: '%45%-2147483648': line number out of range\n")
}

# origin: uutils test_csplit::test_skip_to_match_offset
test test_uu_csplit_skip_to_match_offset { |ctx|
  for offset in ["3", "+3"] {
    let s1 = uu.scene(ctx)?
    uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
    let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", f"%23%{offset}"], stdin: b"")?
    uu.succeeds(r1)
    uu.stdout_only(r1, "75\n")
    let count1 = s1.root.glob("xx*")?.len()
    assert count1 == 1
    uu.file_is(s1, "xx00", generate(26, 51))?
    uu.remove(s1, "xx00")?
  }
}

# origin: uutils test_csplit::test_skip_to_match_offset_suppress_empty
test test_uu_csplit_skip_to_match_offset_suppress_empty { |ctx|
  let s1 = uu.scene(ctx)?
  let r1 = uu.invoke(s1, "csplit", ["-z", "-", "%a%1"], stdin: bytes.from_text("a\n"))?
  uu.succeeds(r1)
  uu.no_output(r1)
  assert ! uu.exists(s1, "xx00")?
}

# origin: uutils test_csplit::test_skip_to_match_option_suppress_matched
test test_uu_csplit_skip_to_match_option_suppress_matched { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "--suppress-matched", "%0$%"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "120\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 1
  uu.file_is(s1, "xx00", generate(11, 51))?
}

# origin: uutils test_csplit::test_skip_to_match_repeat_always
test test_uu_csplit_skip_to_match_repeat_always { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%0$%", "{*}"], stdin: b"")?
  uu.succeeds(r1)
  uu.no_stdout(r1)
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
}

# origin: uutils test_csplit::test_skip_to_match_sequence1
test test_uu_csplit_skip_to_match_sequence1 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%0$%", "%^4%"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "33\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 1
  uu.file_is(s1, "xx00", generate(40, 51))?
}

# origin: uutils test_csplit::test_skip_to_match_sequence2
test test_uu_csplit_skip_to_match_sequence2 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%0$%", "{1}", "%^4%"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "33\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 1
  uu.file_is(s1, "xx00", generate(40, 51))?
}

# origin: uutils test_csplit::test_skip_to_match_sequence3
test test_uu_csplit_skip_to_match_sequence3 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%0$%", "{1}", "/^4/"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "60\n33\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 2
  uu.file_is(s1, "xx00", generate(20, 40))?
  uu.file_is(s1, "xx01", generate(40, 51))?
}

# origin: uutils test_csplit::test_skip_to_match_sequence4
test test_uu_csplit_skip_to_match_sequence4 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%0$%", "/^4/"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "90\n33\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 2
  uu.file_is(s1, "xx00", generate(10, 40))?
  uu.file_is(s1, "xx01", generate(40, 51))?
}

# origin: uutils test_csplit::test_skip_to_no_match1
test test_uu_csplit_skip_to_no_match1 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%nope%"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_only(r1, "csplit: '%nope%': match not found\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
}

# origin: uutils test_csplit::test_skip_to_no_match2
test test_uu_csplit_skip_to_no_match2 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%nope%", "{50}"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_only(r1, "csplit: '%nope%': match not found\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
}

# origin: uutils test_csplit::test_skip_to_no_match3
test test_uu_csplit_skip_to_no_match3 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%0$%", "{50}"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_only(r1, "csplit: '%0$%': match not found on repetition 5\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
}

# origin: uutils test_csplit::test_skip_to_no_match4
test test_uu_csplit_skip_to_no_match4 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%nope%", "/4/"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_only(r1, "csplit: '%nope%': match not found\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
}

# origin: uutils test_csplit::test_skip_to_no_match5
test test_uu_csplit_skip_to_no_match5 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%nope%", "{*}"], stdin: b"")?
  uu.succeeds(r1)
  uu.no_output(r1)
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
}

# origin: uutils test_csplit::test_skip_to_no_match6
test test_uu_csplit_skip_to_no_match6 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%nope%-5"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_only(r1, "csplit: '%nope%-5': match not found\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
}

# origin: uutils test_csplit::test_skip_to_no_match7
test test_uu_csplit_skip_to_no_match7 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "%nope%+5"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_only(r1, "csplit: '%nope%+5': match not found\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
}

# origin: uutils test_csplit::test_stdin
test test_uu_csplit_stdin { |ctx|
  let s1 = uu.scene(ctx)?
  let r1 = uu.invoke(s1, "csplit", ["-", "10"], stdin: bytes.from_text(generate(1, 51)))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "18\n123\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 2
  uu.file_is(s1, "xx00", generate(1, 10))?
  uu.file_is(s1, "xx01", generate(10, 51))?
}

# origin: uutils test_csplit::test_stdin_no_trailing_newline
test test_uu_csplit_stdin_no_trailing_newline { |ctx|
  let s1 = uu.scene(ctx)?
  let r1 = uu.invoke(s1, "csplit", ["-", "2"], stdin: bytes.from_text("a\nb\nc\nd"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "2\n5\n")
}

# origin: uutils test_csplit::test_too_small_line_num
test test_uu_csplit_too_small_line_num { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/20/", "10", "/40/"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "48\n0\n60\n33\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 4
  uu.file_is(s1, "xx00", generate(1, 20))?
  uu.file_is(s1, "xx01", "")?
  uu.file_is(s1, "xx02", generate(20, 40))?
  uu.file_is(s1, "xx03", generate(40, 51))?
}

# origin: uutils test_csplit::test_too_small_line_num_elided
test test_uu_csplit_too_small_line_num_elided { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "-z", "/20/", "10", "/40/"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "48\n60\n33\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 3
  uu.file_is(s1, "xx00", generate(1, 20))?
  uu.file_is(s1, "xx01", generate(20, 40))?
  uu.file_is(s1, "xx02", generate(40, 51))?
}

# origin: uutils test_csplit::test_too_small_line_num_equal
test test_uu_csplit_too_small_line_num_equal { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/20/", "20"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "48\n0\n93\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 3
  uu.file_is(s1, "xx00", generate(1, 20))?
  uu.file_is(s1, "xx01", "")?
  uu.file_is(s1, "xx02", generate(20, 51))?
}

# origin: uutils test_csplit::test_too_small_line_num_negative_offset
test test_uu_csplit_too_small_line_num_negative_offset { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/20/-5", "10", "/40/"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "33\n0\n75\n33\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 4
  uu.file_is(s1, "xx00", generate(1, 15))?
  uu.file_is(s1, "xx01", "")?
  uu.file_is(s1, "xx02", generate(15, 40))?
  uu.file_is(s1, "xx03", generate(40, 51))?
}

# origin: uutils test_csplit::test_too_small_line_num_repeat
test test_uu_csplit_too_small_line_num_repeat { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/20/", "10", "{*}"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_is(r1, "csplit: '10': line number out of range on repetition 5\n")
  uu.stdout_is(r1, "48\n0\n0\n30\n30\n30\n3\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
  let s2 = uu.scene(ctx)?
  uu.fixture(s2, "csplit", "numbers50.txt", "numbers50.txt")?
  let r2 = uu.invoke(s2, "csplit", ["numbers50.txt", "/20/", "10", "{*}", "-k"], stdin: b"")?
  uu.fails(r2)
  uu.stderr_is(r2, "csplit: '10': line number out of range on repetition 5\n")
  uu.stdout_is(r2, "48\n0\n0\n30\n30\n30\n3\n")
  let count2 = s2.root.glob("xx*")?.len()
  assert count2 == 7
  uu.file_is(s2, "xx00", generate(1, 20))?
  uu.file_is(s2, "xx01", "")?
  uu.file_is(s2, "xx02", "")?
  uu.file_is(s2, "xx03", generate(20, 30))?
  uu.file_is(s2, "xx04", generate(30, 40))?
  uu.file_is(s2, "xx05", generate(40, 50))?
  uu.file_is(s2, "xx06", "50\n")?
}

# origin: uutils test_csplit::test_too_small_line_num_twice
test test_uu_csplit_too_small_line_num_twice { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/20/", "10", "15", "/40/"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "48\n0\n0\n60\n33\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 5
  uu.file_is(s1, "xx00", generate(1, 20))?
  uu.file_is(s1, "xx01", "")?
  uu.file_is(s1, "xx02", "")?
  uu.file_is(s1, "xx03", generate(20, 40))?
  uu.file_is(s1, "xx04", generate(40, 51))?
}

# origin: uutils test_csplit::test_up_to_line
test test_uu_csplit_up_to_line { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "10"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "18\n123\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 2
  uu.file_is(s1, "xx00", generate(1, 10))?
  uu.file_is(s1, "xx01", generate(10, 51))?
}

# origin: uutils test_csplit::test_up_to_line_option_suppress_matched
test test_uu_csplit_up_to_line_option_suppress_matched { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "--suppress-matched", "10"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "18\n120\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 2
  uu.file_is(s1, "xx00", generate(1, 10))?
  uu.file_is(s1, "xx01", generate(11, 51))?
}

# origin: uutils test_csplit::test_up_to_line_repeat_twice
test test_uu_csplit_up_to_line_repeat_twice { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "10", "{2}"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "18\n30\n30\n63\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 4
  uu.file_is(s1, "xx00", generate(1, 10))?
  uu.file_is(s1, "xx01", generate(10, 20))?
  uu.file_is(s1, "xx02", generate(20, 30))?
  uu.file_is(s1, "xx03", generate(30, 51))?
}

# origin: uutils test_csplit::test_up_to_line_sequence
test test_uu_csplit_up_to_line_sequence { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "10", "25"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "18\n45\n78\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 3
  uu.file_is(s1, "xx00", generate(1, 10))?
  uu.file_is(s1, "xx01", generate(10, 25))?
  uu.file_is(s1, "xx02", generate(25, 51))?
}


# origin: uutils test_csplit::test_up_to_match
test test_uu_csplit_up_to_match { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/9$/"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "16\n125\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 2
  uu.file_is(s1, "xx00", generate(1, 9))?
  uu.file_is(s1, "xx01", generate(9, 51))?
}

# origin: uutils test_csplit::test_up_to_match_context_overflow
test test_uu_csplit_up_to_match_context_overflow { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/45/+10"], stdin: b"")?
  uu.fails(r1)
  uu.stdout_is(r1, "141\n")
  uu.stderr_is(r1, "csplit: '/45/+10': line number out of range\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
  let s2 = uu.scene(ctx)?
  uu.fixture(s2, "csplit", "numbers50.txt", "numbers50.txt")?
  let r2 = uu.invoke(s2, "csplit", ["numbers50.txt", "/45/+10", "-k"], stdin: b"")?
  uu.fails(r2)
  uu.stdout_is(r2, "141\n")
  uu.stderr_is(r2, "csplit: '/45/+10': line number out of range\n")
  let count2 = s2.root.glob("xx*")?.len()
  assert count2 == 1
  uu.file_is(s2, "xx00", generate(1, 51))?
}

# origin: uutils test_csplit::test_up_to_match_context_underflow
test test_uu_csplit_up_to_match_context_underflow { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/5/-10"], stdin: b"")?
  uu.fails(r1)
  uu.stdout_is(r1, "0\n")
  uu.stderr_is(r1, "csplit: '/5/-10': line number out of range\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 0
  let s2 = uu.scene(ctx)?
  uu.fixture(s2, "csplit", "numbers50.txt", "numbers50.txt")?
  let r2 = uu.invoke(s2, "csplit", ["numbers50.txt", "/5/-10", "-k"], stdin: b"")?
  uu.fails(r2)
  uu.stdout_is(r2, "0\n")
  uu.stderr_is(r2, "csplit: '/5/-10': line number out of range\n")
  let count2 = s2.root.glob("xx*")?.len()
  assert count2 == 1
  uu.file_is(s2, "xx00", "")?
}

# origin: uutils test_csplit::test_up_to_match_negative_offset
test test_uu_csplit_up_to_match_negative_offset { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/9$/-3"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "10\n131\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 2
  uu.file_is(s1, "xx00", generate(1, 6))?
  uu.file_is(s1, "xx01", generate(6, 51))?
}

# origin: uutils test_csplit::test_up_to_match_negative_offset_at_second_split_start
test test_uu_csplit_up_to_match_negative_offset_at_second_split_start { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/10$/", "/12$/-2"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "18\n0\n123\n")
  uu.file_is(s1, "xx00", generate(1, 10))?
  uu.file_is(s1, "xx01", "")?
  uu.file_is(s1, "xx02", generate(10, 51))?
}

# origin: uutils test_csplit::test_up_to_match_negative_offset_at_split_start
test test_uu_csplit_up_to_match_negative_offset_at_split_start { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/3$/-2"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0\n141\n")
  uu.file_is(s1, "xx00", "")?
  uu.file_is(s1, "xx01", generate(1, 51))?
}

# origin: uutils test_csplit::test_up_to_match_negative_offset_at_split_start_suppress_matched
test test_uu_csplit_up_to_match_negative_offset_at_split_start_suppress_matched { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["--suppress-matched", "numbers50.txt", "/10$/", "/12$/-1"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "18\n0\n117\n")
  uu.file_is(s1, "xx00", generate(1, 10))?
  uu.file_is(s1, "xx01", "")?
  uu.file_is(s1, "xx02", generate(12, 51))?
}

# origin: uutils test_csplit::test_up_to_match_negative_offset_before_second_split_start
test test_uu_csplit_up_to_match_negative_offset_before_second_split_start { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/10$/", "/12$/-3"], stdin: b"")?
  uu.fails(r1)
  uu.stdout_is(r1, "18\n0\n")
  uu.stderr_is(r1, "csplit: '/12$/-3': line number out of range\n")
  assert s1.root.glob("xx*")?.len() == 0
}

# origin: uutils test_csplit::test_up_to_match_negative_offset_before_split_start
test test_uu_csplit_up_to_match_negative_offset_before_split_start { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/3$/-3"], stdin: b"")?
  uu.fails(r1)
  uu.stdout_is(r1, "0\n")
  uu.stderr_is(r1, "csplit: '/3$/-3': line number out of range\n")
  assert s1.root.glob("xx*")?.len() == 0
}

# origin: uutils test_csplit::test_up_to_match_negative_offset_before_split_start_keep_files
test test_uu_csplit_up_to_match_negative_offset_before_split_start_keep_files { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["-k", "numbers50.txt", "/3$/-3"], stdin: b"")?
  uu.fails(r1)
  uu.stdout_is(r1, "0\n")
  uu.stderr_is(r1, "csplit: '/3$/-3': line number out of range\n")
  uu.file_is(s1, "xx00", "")?
}

# origin: uutils test_csplit::test_up_to_match_negative_offset_before_split_start_suppress_matched
test test_uu_csplit_up_to_match_negative_offset_before_split_start_suppress_matched { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["--suppress-matched", "numbers50.txt", "/10$/", "/12$/-2"], stdin: b"")?
  uu.fails(r1)
  uu.stdout_is(r1, "18\n0\n")
  uu.stderr_is(r1, "csplit: '/12$/-2': line number out of range\n")
  assert s1.root.glob("xx*")?.len() == 0
}

# origin: uutils test_csplit::test_up_to_match_negative_offset_min_i32
test test_uu_csplit_up_to_match_negative_offset_min_i32 { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "/45/-2147483648"], stdin: b"")?
  uu.fails(r1)
  uu.stderr_is(r1, "csplit: '/45/-2147483648': line number out of range\n")
}

# origin: uutils test_csplit::test_up_to_match_negative_offset_option_suppress_matched
test test_uu_csplit_up_to_match_negative_offset_option_suppress_matched { |ctx|
  let s1 = uu.scene(ctx)?
  uu.fixture(s1, "csplit", "numbers50.txt", "numbers50.txt")?
  let r1 = uu.invoke(s1, "csplit", ["numbers50.txt", "--suppress-matched", "/10/-4"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "10\n129\n")
  let count1 = s1.root.glob("xx*")?.len()
  assert count1 == 2
  uu.file_is(s1, "xx00", generate(1, 6))?
  uu.file_is(s1, "xx01", generate(7, 51))?
}

# origin: uutils test_csplit::test_named_pipe_input_file
test test_uu_csplit_named_pipe_input_file { |ctx|
  let s = uu.scene(ctx)?
  fs.mkfifo(uu.at(s, "fifo"), 0o700)?
  let writer_plan = process.command_argv(p"/bin/sh",
    ["sh", "-c", "printf '%s' \"$1\" > \"$2\"", "writer", generate(1, 51), uu.at(s, "fifo").display()],
    s.root, {}, b"", uu.at(s, "writer-out"), uu.at(s, "writer-err"), timeout: 5s)
  let writer = spawn writer_plan?
  defer writer.cancel(signal: "KILL", kill_after: 0ms)?
  let r = uu.invoke(s, "csplit", ["fifo", "10"], timeout: 5s)?
  let _ = wait writer?
  uu.succeeds(r)
  uu.stdout_only(r, "18\n123\n")
  assert s.root.glob("xx*")?.len() == 2
  uu.file_is(s, "xx00", generate(1, 10))?
  uu.file_is(s, "xx01", generate(10, 51))?
}

# origin: uutils test_csplit::test_directory_input_file
test test_uu_csplit_directory_input_file { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "test_directory")?
  let r = uu.invoke(s, "csplit", ["test_directory", "1"])?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "0\n")
  uu.stderr_is(r, "csplit: read error: Is a directory\n")
}

# origin: uutils test_csplit::test_csplit_non_utf8_paths
test test_uu_csplit_csplit_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let filename = uu.at_bytes(s, b"\xff\xfe")?
  filename.write(b"line1\nline2\nline3\nline4\nline5\n")?
  let r = uu.invoke_paths(s, "csplit", [Path.parse_bytes(b"\xff\xfe")?, p"3"])?
  uu.succeeds(r)
}

# origin: uutils test_csplit::test_create_error_reports_filename
test test_uu_csplit_create_error_reports_filename { |ctx|
  if applet.current_euid() == 0 { test.skip("root can write a mode-000 file") }
  let s = uu.scene(ctx)?
  uu.write(s, "input", "a\nb\nc\n")?
  uu.touch(s, "xx00")?
  uu.set_mode(s, "xx00", 0o000)?
  let r = uu.invoke(s, "csplit", ["input", "2"])?
  uu.fails(r)
  uu.stderr_is(r, "csplit: xx00: Permission denied\n")
}

# origin: uutils test_csplit::test_csplit_dev_full_stdout
test test_uu_csplit_csplit_dev_full_stdout { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "csplit", ["/etc/hosts", "1"], stdout: p"/dev/full")?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "csplit: write error: No space left on device\n")
}

# origin: uutils test_csplit::test_up_to_line_with_non_ascii_repeat
test test_uu_csplit_up_to_line_with_non_ascii_repeat { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "csplit", "numbers50.txt", "numbers50.txt")?
  let r = uu.invoke(s, "csplit", ["numbers50.txt", "10", "{𝟚}"])?
  uu.fails(r)
  uu.stderr_contains(r, "integer required between '{' and '}'")
}
