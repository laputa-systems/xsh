##! Transcribed from the MIT-licensed uutils csplit integration tests.

use support.uu as uu

pure numbers(from: Int, to: Int) -> Str {
  [f"{number}\n" for number in range(from, to)].join("")
}

proc scene(ctx: TestContext) [fs, error] -> Result[uu.Scene, Error] {
  let s = uu.scene(ctx)?
  uu.fixture(s, "csplit", "numbers50.txt", "numbers50.txt")?
  Ok(s)
}

proc split_count(s: uu.Scene) [fs, error] -> Result[Int, Error] {
  let count = fs.children(s.root)? |> where { |entry| entry.name.starts_with("xx") } |> count()
  Ok(count)
}

# Missing paths are not regular files, matching the upstream scene helper.
proc file_exists(s: uu.Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  let file = uu.at(s, name)
  if !file.exists()? { return Ok(false) }
  Ok(file.is_file()?)
}

# origin: uutils test_csplit::test_up_to_match_negative_offset_repeat_twice
test test_uu_csplit_up_to_match_negative_offset_repeat_twice { |ctx|
  let s = scene(ctx)?
  let r = uu.invoke(s, "csplit", ["numbers50.txt", "/9$/-3", "{2}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "10\n26\n30\n75\n")
  assert split_count(s)? == 4
  uu.file_is(s, "xx00", numbers(1, 6))
  uu.file_is(s, "xx01", numbers(6, 16))
  uu.file_is(s, "xx02", numbers(16, 26))
  uu.file_is(s, "xx03", numbers(26, 51))
}

# origin: uutils test_csplit::test_up_to_match_non_ascii_offset
test test_uu_csplit_up_to_match_non_ascii_offset { |ctx|
  let s = scene(ctx)?
  let r = uu.invoke(s, "csplit", ["numbers50.txt", "/9$/𝟚"])?
  uu.fails(r)
  uu.stderr_contains(r, "integer expected after delimiter")
}

# origin: uutils test_csplit::test_up_to_match_offset
test test_uu_csplit_up_to_match_offset { |ctx|
  for offset in ["3", "+3"] {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", f"/9$/{offset}"])?
    uu.succeeds(r)
    uu.stdout_only(r, "24\n117\n")
    assert split_count(s)? == 2
    uu.file_is(s, "xx00", numbers(1, 12))
    uu.file_is(s, "xx01", numbers(12, 51))
    uu.remove(s, "xx00")?
    uu.remove(s, "xx01")?
  }
}

# origin: uutils test_csplit::test_up_to_match_offset_final_empty
test test_uu_csplit_up_to_match_offset_final_empty { |ctx|
  let s = scene(ctx)?
  let r = uu.invoke(s, "csplit", ["-", "/a/+1"], stdin: b"1\na\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "4\n0\n")
  assert split_count(s)? == 2
  uu.file_is(s, "xx00", "1\na\n")
  uu.file_is(s, "xx01", "")
}

# origin: uutils test_csplit::test_up_to_match_offset_option_suppress_matched
test test_uu_csplit_up_to_match_offset_option_suppress_matched { |ctx|
  let s = scene(ctx)?
  let r = uu.invoke(s, "csplit", ["numbers50.txt", "--suppress-matched", "/10/+4"])?
  uu.succeeds(r)
  uu.stdout_only(r, "30\n108\n")
  assert split_count(s)? == 2
  uu.file_is(s, "xx00", numbers(1, 14))
  uu.file_is(s, "xx01", numbers(15, 51))
}

# origin: uutils test_csplit::test_up_to_match_offset_repeat_twice
test test_uu_csplit_up_to_match_offset_repeat_twice { |ctx|
  let s = scene(ctx)?
  let r = uu.invoke(s, "csplit", ["numbers50.txt", "/9$/+3", "{2}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "24\n30\n30\n57\n")
  assert split_count(s)? == 4
  uu.file_is(s, "xx00", numbers(1, 12))
  uu.file_is(s, "xx01", numbers(12, 22))
  uu.file_is(s, "xx02", numbers(22, 32))
  uu.file_is(s, "xx03", numbers(32, 51))
}

# origin: uutils test_csplit::test_up_to_match_option_suppress_matched
test test_uu_csplit_up_to_match_option_suppress_matched { |ctx|
  let s = scene(ctx)?
  let r = uu.invoke(s, "csplit", ["numbers50.txt", "--suppress-matched", "/0$/", "{*}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "18\n27\n27\n27\n27\n0\n")
  assert split_count(s)? == 6
  uu.file_is(s, "xx00", numbers(1, 10))
  uu.file_is(s, "xx01", numbers(11, 20))
  uu.file_is(s, "xx02", numbers(21, 30))
  uu.file_is(s, "xx03", numbers(31, 40))
  uu.file_is(s, "xx04", numbers(41, 50))
  uu.file_is(s, "xx05", "")
}

# origin: uutils test_csplit::test_up_to_match_repeat_always
test test_uu_csplit_up_to_match_repeat_always { |ctx|
  let s = scene(ctx)?
  let r = uu.invoke(s, "csplit", ["numbers50.txt", "/9$/", "{*}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "16\n29\n30\n30\n30\n6\n")
  assert split_count(s)? == 6
  uu.file_is(s, "xx00", numbers(1, 9))
  uu.file_is(s, "xx01", numbers(9, 19))
  uu.file_is(s, "xx02", numbers(19, 29))
  uu.file_is(s, "xx03", numbers(29, 39))
  uu.file_is(s, "xx04", numbers(39, 49))
  uu.file_is(s, "xx05", numbers(49, 51))
}

# origin: uutils test_csplit::test_up_to_match_repeat_over
test test_uu_csplit_up_to_match_repeat_over { |ctx|
  {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", "/9$/", "{50}"])?
    uu.fails(r)
    uu.stdout_is(r, "16\n29\n30\n30\n30\n6\n")
    uu.stderr_is(r, "csplit: '/9$/': match not found on repetition 5\n")
    assert split_count(s)? == 0
  }
  {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", "/9$/", "{50}", "-k"])?
    uu.fails(r)
    uu.stdout_is(r, "16\n29\n30\n30\n30\n6\n")
    uu.stderr_is(r, "csplit: '/9$/': match not found on repetition 5\n")
    assert split_count(s)? == 6
    uu.file_is(s, "xx00", numbers(1, 9))
    uu.file_is(s, "xx01", numbers(9, 19))
    uu.file_is(s, "xx02", numbers(19, 29))
    uu.file_is(s, "xx03", numbers(29, 39))
    uu.file_is(s, "xx04", numbers(39, 49))
    uu.file_is(s, "xx05", numbers(49, 51))
  }
}

# origin: uutils test_csplit::test_up_to_match_repeat_twice
test test_uu_csplit_up_to_match_repeat_twice { |ctx|
  let s = scene(ctx)?
  let r = uu.invoke(s, "csplit", ["numbers50.txt", "/9$/", "{2}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "16\n29\n30\n66\n")
  assert split_count(s)? == 4
  uu.file_is(s, "xx00", numbers(1, 9))
  uu.file_is(s, "xx01", numbers(9, 19))
  uu.file_is(s, "xx02", numbers(19, 29))
  uu.file_is(s, "xx03", numbers(29, 51))
}

# origin: uutils test_csplit::test_up_to_match_sequence
test test_uu_csplit_up_to_match_sequence { |ctx|
  let s = scene(ctx)?
  let r = uu.invoke(s, "csplit", ["numbers50.txt", "/9$/", "/5$/"])?
  uu.succeeds(r)
  uu.stdout_only(r, "16\n17\n108\n")
  assert split_count(s)? == 3
  uu.file_is(s, "xx00", numbers(1, 9))
  uu.file_is(s, "xx01", numbers(9, 15))
  uu.file_is(s, "xx02", numbers(15, 51))
}

# origin: uutils test_csplit::test_up_to_match_suppress_matched_final_empty
test test_uu_csplit_up_to_match_suppress_matched_final_empty { |ctx|
  let s = scene(ctx)?
  let r = uu.invoke(s, "csplit", ["--suppress-matched", "-", "2", "/a/"], stdin: b"1\n2\n3\n4\na\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n4\n0\n")
  assert split_count(s)? == 3
  uu.file_is(s, "xx00", "1\n")
  uu.file_is(s, "xx01", "3\n4\n")
  uu.file_is(s, "xx02", "")
}

# origin: uutils test_csplit::test_up_to_no_match1
test test_uu_csplit_up_to_no_match1 { |ctx|
  {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", "/4/", "/nope/"])?
    uu.fails(r)
    uu.stdout_is(r, "6\n135\n")
    uu.stderr_is(r, "csplit: '/nope/': match not found\n")
    assert split_count(s)? == 0
  }
  {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", "/4/", "/nope/", "-k"])?
    uu.fails(r)
    uu.stdout_is(r, "6\n135\n")
    uu.stderr_is(r, "csplit: '/nope/': match not found\n")
    assert split_count(s)? == 2
    uu.file_is(s, "xx00", numbers(1, 4))
    uu.file_is(s, "xx01", numbers(4, 51))
  }
}

# origin: uutils test_csplit::test_up_to_no_match2
test test_uu_csplit_up_to_no_match2 { |ctx|
  {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", "/4/", "/nope/", "{50}"])?
    uu.fails(r)
    uu.stdout_is(r, "6\n135\n")
    uu.stderr_is(r, "csplit: '/nope/': match not found\n")
    assert split_count(s)? == 0
  }
  {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", "/4/", "/nope/", "{50}", "-k"])?
    uu.fails(r)
    uu.stdout_is(r, "6\n135\n")
    uu.stderr_is(r, "csplit: '/nope/': match not found\n")
    assert split_count(s)? == 2
    uu.file_is(s, "xx00", numbers(1, 4))
    uu.file_is(s, "xx01", numbers(4, 51))
  }
}

# origin: uutils test_csplit::test_up_to_no_match3
test test_uu_csplit_up_to_no_match3 { |ctx|
  {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", "/0$/", "{50}"])?
    uu.fails(r)
    uu.stdout_is(r, "18\n30\n30\n30\n30\n3\n")
    uu.stderr_is(r, "csplit: '/0$/': match not found on repetition 5\n")
    assert split_count(s)? == 0
  }
  {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", "/0$/", "{50}", "-k"])?
    uu.fails(r)
    uu.stdout_is(r, "18\n30\n30\n30\n30\n3\n")
    uu.stderr_is(r, "csplit: '/0$/': match not found on repetition 5\n")
    assert split_count(s)? == 6
    uu.file_is(s, "xx00", numbers(1, 10))
    uu.file_is(s, "xx01", numbers(10, 20))
    uu.file_is(s, "xx02", numbers(20, 30))
    uu.file_is(s, "xx03", numbers(30, 40))
    uu.file_is(s, "xx04", numbers(40, 50))
    uu.file_is(s, "xx05", "50\n")
  }
}

# origin: uutils test_csplit::test_up_to_no_match4
test test_uu_csplit_up_to_no_match4 { |ctx|
  {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", "/nope/", "/4/"])?
    uu.fails(r)
    uu.stdout_is(r, "141\n")
    uu.stderr_is(r, "csplit: '/nope/': match not found\n")
    assert split_count(s)? == 0
  }
  {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", "/nope/", "/4/", "-k"])?
    uu.fails(r)
    uu.stdout_is(r, "141\n")
    uu.stderr_is(r, "csplit: '/nope/': match not found\n")
    assert split_count(s)? == 1
    uu.file_is(s, "xx00", numbers(1, 51))
  }
}

# origin: uutils test_csplit::test_up_to_no_match5
test test_uu_csplit_up_to_no_match5 { |ctx|
  let s = scene(ctx)?
  let r = uu.invoke(s, "csplit", ["numbers50.txt", "/nope/", "{*}"])?
  uu.succeeds(r)
  uu.stdout_only(r, "141\n")
  assert split_count(s)? == 1
  uu.file_is(s, "xx00", numbers(1, 51))
}

# origin: uutils test_csplit::test_up_to_no_match6
test test_uu_csplit_up_to_no_match6 { |ctx|
  {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", "/nope/-5"])?
    uu.fails(r)
    uu.stdout_is(r, "141\n")
    uu.stderr_is(r, "csplit: '/nope/-5': match not found\n")
    assert split_count(s)? == 0
  }
  {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", "/nope/-5", "-k"])?
    uu.fails(r)
    uu.stdout_is(r, "141\n")
    uu.stderr_is(r, "csplit: '/nope/-5': match not found\n")
    assert split_count(s)? == 1
    uu.file_is(s, "xx00", numbers(1, 51))
  }
}

# origin: uutils test_csplit::test_up_to_no_match7
test test_uu_csplit_up_to_no_match7 { |ctx|
  {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", "/nope/+5"])?
    uu.fails(r)
    uu.stdout_is(r, "141\n")
    uu.stderr_is(r, "csplit: '/nope/+5': match not found\n")
    assert split_count(s)? == 0
  }
  {
    let s = scene(ctx)?
    let r = uu.invoke(s, "csplit", ["numbers50.txt", "/nope/+5", "-k"])?
    uu.fails(r)
    uu.stdout_is(r, "141\n")
    uu.stderr_is(r, "csplit: '/nope/+5': match not found\n")
    assert split_count(s)? == 1
    uu.file_is(s, "xx00", numbers(1, 51))
  }
}

# origin: uutils test_csplit::test_write_error_dev_full
test test_uu_csplit_write_error_dev_full { |ctx|
  let s = scene(ctx)?
  uu.symlink(s, "/dev/full", "xx01")?
  let r = uu.invoke(s, "csplit", ["-", "2"], stdin: b"1\n2\n")?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "xx01: No space left on device")
  assert !file_exists(s, "xx00")?
}

# origin: uutils test_csplit::test_write_error_dev_full_keep_files
test test_uu_csplit_write_error_dev_full_keep_files { |ctx|
  let s = scene(ctx)?
  uu.symlink(s, "/dev/full", "xx01")?
  let r = uu.invoke(s, "csplit", ["-k", "-", "2"], stdin: b"1\n2\n")?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "xx01: No space left on device")
  uu.file_is(s, "xx00", "1\n")
  assert file_exists(s, "xx00")?
}

# origin: uutils test_csplit::zero_error
test test_uu_csplit_zero_error { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "in")?
  let r = uu.invoke(s, "csplit", ["in", "0"])?
  uu.fails(r)
  uu.stderr_contains(r, "0: line number must be greater")
}

# origin: uutils test_csplit::zero_precision_format
test test_uu_csplit_zero_precision_format { |ctx|
  let s = scene(ctx)?
  let r = uu.invoke(s, "csplit", ["numbers50.txt", "10", "--suffix-format", "%.0d"])?
  uu.succeeds(r)
  uu.stdout_only(r, "18\n123\n")
  assert split_count(s)? == 2
  uu.file_is(s, "xx", numbers(1, 10))
  uu.file_is(s, "xx1", numbers(10, 51))
}

