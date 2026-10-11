##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_comm.rs.
use support.uu as uu

# origin: uutils test_comm::a_empty
test test_uu_comm_a_empty { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.touch(s, "empty")?
  let r1 = uu.invoke(s, "comm", ["a", "empty"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\nz\n")
}

# origin: uutils test_comm::ab_dash_one
test test_uu_comm_ab_dash_one { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.write(s, "b", "b\nz\n")?
  let r1 = uu.invoke(s, "comm", ["a", "b", "-1"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "b\n\tz\n")
}

# origin: uutils test_comm::ab_dash_three
test test_uu_comm_ab_dash_three { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.write(s, "b", "b\nz\n")?
  let r1 = uu.invoke(s, "comm", ["a", "b", "-3"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\n\tb\n")
}

# origin: uutils test_comm::ab_dash_two
test test_uu_comm_ab_dash_two { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.write(s, "b", "b\nz\n")?
  let r1 = uu.invoke(s, "comm", ["a", "b", "-2"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\n\tz\n")
}

# origin: uutils test_comm::ab_no_args
test test_uu_comm_ab_no_args { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.write(s, "b", "b\nz\n")?
  let r1 = uu.invoke(s, "comm", ["a", "b"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\n\tb\n\t\tz\n")
}

# origin: uutils test_comm::check_order
test test_uu_comm_check_order { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "bad_order_1", "e\nd\nb\na\n")?
  uu.write(s, "bad_order_2", "e\nc\nb\na\n")?
  let r1 = uu.invoke(s, "comm", ["--check-order", "bad_order_1", "bad_order_2"])?
  uu.fails(r1)
  uu.stdout_is(r1, "\t\te\n")
  uu.stderr_is(r1, "comm: file 1 is not in sorted order\n")
}

# origin: uutils test_comm::comm_emoji_sorted_inputs
test test_uu_comm_comm_emoji_sorted_inputs { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "💐\n🦀\n")?
  uu.write(s, "file2", "🦀\n🪽\n")?
  let r1 = uu.invoke(s, "comm", ["file1", "file2"], vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r1)
  uu.stdout_only(r1, "💐\n\t\t🦀\n\t🪽\n")
}

# origin: uutils test_comm::defaultcheck_order
test test_uu_comm_defaultcheck_order { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\n")?
  uu.write(s, "bad_order_1", "e\nd\nb\na\n")?
  let r1 = uu.invoke(s, "comm", ["a", "bad_order_1"])?
  uu.fails(r1)
  uu.stdout_is(r1, "a\n\te\n\td\n\tb\n\ta\n")
  uu.stderr_is(r1, "comm: file 2 is not in sorted order\ncomm: input is not in sorted order\n",)
}

# origin: uutils test_comm::defaultcheck_order_identical_bad_order_files
test test_uu_comm_defaultcheck_order_identical_bad_order_files { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "bad_order_1", "e\nd\nb\na\n")?
  let r1 = uu.invoke(s, "comm", ["bad_order_1", "bad_order_1"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t\te\n\t\td\n\t\tb\n\t\ta\n")
  let r2 = uu.invoke(s, "comm", ["--check-order", "bad_order_1", "bad_order_1"])?
  uu.fails(r2)
  uu.stdout_is(r2, "\t\te\n")
  uu.stderr_is(r2, "comm: file 1 is not in sorted order\n")
}

# origin: uutils test_comm::empty_empty
test test_uu_comm_empty_empty { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "empty")?
  let r1 = uu.invoke(s, "comm", ["empty", "empty"])?
  uu.succeeds(r1)
  uu.no_output(r1)
}

# origin: uutils test_comm::no_arguments
test test_uu_comm_no_arguments { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "comm", [])?
  uu.fails(r1)
  uu.no_stdout(r1)
}

# origin: uutils test_comm::nocheck_order
test test_uu_comm_nocheck_order { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "bad_order_1", "e\nd\nb\na\n")?
  uu.write(s, "bad_order_2", "e\nc\nb\na\n")?
  let r1 = uu.invoke(s, "comm", ["--nocheck-order", "bad_order_1", "bad_order_2"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t\te\n\tc\n\tb\n\ta\nd\nb\na\n")
  uu.no_stderr(r1)
}

# origin: uutils test_comm::one_argument
test test_uu_comm_one_argument { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "comm", ["a"])?
  uu.fails(r1)
  uu.no_stdout(r1)
}

# origin: uutils test_comm::output_delimiter
test test_uu_comm_output_delimiter { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.write(s, "b", "b\nz\n")?
  let r1 = uu.invoke(s, "comm", ["--output-delimiter=word", "a", "b"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\nwordb\nwordwordz\n")
}

# origin: uutils test_comm::output_delimiter_hyphen_help
test test_uu_comm_output_delimiter_hyphen_help { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.write(s, "b", "b\nz\n")?
  let r1 = uu.invoke(s, "comm", ["--output-delimiter", "--help", "a", "b"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\n--helpb\n--help--helpz\n")
}

# origin: uutils test_comm::output_delimiter_hyphen_one
test test_uu_comm_output_delimiter_hyphen_one { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.write(s, "b", "b\nz\n")?
  let r1 = uu.invoke(s, "comm", ["--output-delimiter", "-1", "a", "b"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\n-1b\n-1-1z\n")
}

# origin: uutils test_comm::output_delimiter_multiple_different
test test_uu_comm_output_delimiter_multiple_different { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.write(s, "b", "b\nz\n")?
  let r1 = uu.invoke(s, "comm", ["--output-delimiter=word", "--output-delimiter=other", "a", "b"])?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "multiple")
  uu.stderr_contains(r1, "output")
  uu.stderr_contains(r1, "delimiters")
}

# origin: uutils test_comm::output_delimiter_multiple_identical
test test_uu_comm_output_delimiter_multiple_identical { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.write(s, "b", "b\nz\n")?
  let r1 = uu.invoke(s, "comm", ["--output-delimiter=word", "--output-delimiter=word", "a", "b"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\nwordb\nwordwordz\n")
}

# origin: uutils test_comm::output_delimiter_nul
test test_uu_comm_output_delimiter_nul { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.write(s, "b", "b\nz\n")?
  let r1 = uu.invoke(s, "comm", ["--output-delimiter=", "a", "b"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\n\0b\n\0\0z\n")
}

# origin: uutils test_comm::repeated_flags
test test_uu_comm_repeated_flags { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.write(s, "b", "b\nz\n")?
  let r1 = uu.invoke(s, "comm", ["--total", "-123123", "--total", "a", "b"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1\t1\t1\ttotal\n")
}

# origin: uutils test_comm::test_both_inputs_out_of_order
test test_uu_comm_both_inputs_out_of_order { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file_a", "3\n1\n0\n")?
  uu.write(s, "file_b", "3\n2\n0\n")?
  let r1 = uu.invoke(s, "comm", ["file_a", "file_b"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_is(r1, "\t\t3\n1\n0\n\t2\n\t0\n")
  uu.stderr_is(r1, "comm: file 1 is not in sorted order\ncomm: file 2 is not in sorted order\ncomm: input is not in sorted order\n",)
}

# origin: uutils test_comm::test_both_inputs_out_of_order_but_identical
test test_uu_comm_both_inputs_out_of_order_but_identical { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file_a", "2\n1\n0\n")?
  uu.write(s, "file_b", "2\n1\n0\n")?
  let r1 = uu.invoke(s, "comm", ["file_a", "file_b"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t\t2\n\t\t1\n\t\t0\n")
  uu.no_stderr(r1)
}

# origin: uutils test_comm::test_both_inputs_out_of_order_last_pair
test test_uu_comm_both_inputs_out_of_order_last_pair { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file_a", "3\n1\n")?
  uu.write(s, "file_b", "3\n2\n")?
  let r1 = uu.invoke(s, "comm", ["file_a", "file_b"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_is(r1, "\t\t3\n1\n\t2\n")
  uu.stderr_is(r1, "comm: file 1 is not in sorted order\ncomm: file 2 is not in sorted order\ncomm: input is not in sorted order\n",)
}

# origin: uutils test_comm::test_c_locale_still_orders_by_bytes
test test_uu_comm_c_locale_still_orders_by_bytes { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f1", "a-b\na1\n")?
  uu.write(s, "f2", "a-b\n")?
  let r1 = uu.invoke(s, "comm", ["-12", "f1", "f2"], vars: {LC_ALL: "C"})?
  uu.succeeds(r1)
  uu.stdout_only(r1, "a-b\n")
}

# origin: uutils test_comm::test_comm_eintr_handling
test test_uu_comm_comm_eintr_handling { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "line1\nline2\nline3\n")?
  uu.write(s, "file2", "line1\nline2\nline3\n")?
  let r1 = uu.invoke(s, "comm", ["file1", "file2"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "line1")
  uu.stdout_contains(r1, "line2")
  uu.stdout_contains(r1, "line3")
  uu.write(s, "file1", "line1\nline2\nline3\n")?
  uu.write(s, "file2", "line1\nline2\nline3\n")?
  let r2 = uu.invoke(s, "comm", ["file1", "file2"])?
  uu.succeeds(r2)
  uu.stdout_contains(r2, "line1")
  uu.stdout_contains(r2, "line2")
  uu.stdout_contains(r2, "line3")
}

# origin: uutils test_comm::test_comm_write_error_dev_full
test test_uu_comm_comm_write_error_dev_full { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\n")?
  let r1 = uu.invoke(s, "comm", ["a", "a"], stdout: p"/dev/full")?
  uu.fails(r1)
  uu.stderr_is(r1, "comm: write error: No space left on device\n")
}

# origin: uutils test_comm::test_first_input_out_of_order_extended
test test_uu_comm_first_input_out_of_order_extended { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file_a", "0\n3\n1\n")?
  uu.write(s, "file_b", "2\n3\n")?
  let r1 = uu.invoke(s, "comm", ["file_a", "file_b"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_is(r1, "0\n\t2\n\t\t3\n1\n")
  uu.stderr_is(r1, "comm: file 1 is not in sorted order\ncomm: input is not in sorted order\n",)
}

# origin: uutils test_comm::test_identical_unsorted_prefix_check_order_fails
test test_uu_comm_identical_unsorted_prefix_check_order_fails { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f1", "b\na\nc\n")?
  uu.write(s, "f2", "b\na\nd\n")?
  let r1 = uu.invoke(s, "comm", ["--check-order", "f1", "f2"])?
  uu.fails(r1)
  uu.stdout_is(r1, "\t\tb\n")
  uu.stderr_is(r1, "comm: file 1 is not in sorted order\n")
}

# origin: uutils test_comm::test_identical_unsorted_prefix_no_error
test test_uu_comm_identical_unsorted_prefix_no_error { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f1", "b\na\nc\n")?
  uu.write(s, "f2", "b\na\nd\n")?
  let r1 = uu.invoke(s, "comm", ["f1", "f2"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t\tb\n\t\ta\nc\n\td\n")
  uu.no_stderr(r1)
}

# origin: uutils test_comm::test_invalid_arg
test test_uu_comm_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "comm", ["--definitely-invalid"])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_comm::test_is_dir
test test_uu_comm_is_dir { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "comm", [".", "."])?
  uu.fails(r1)
  uu.stderr_only(r1, "comm: .: Is a directory\n")
  uu.mkdir(s, "dir")?
  let r2 = uu.invoke(s, "comm", ["dir", "."])?
  uu.fails(r2)
  uu.stderr_only(r2, "comm: dir: Is a directory\n")
  uu.touch(s, "file")?
  let r3 = uu.invoke(s, "comm", [".", "file"])?
  uu.fails(r3)
  uu.stderr_only(r3, "comm: .: Is a directory\n")
  uu.touch(s, "file")?
  let r4 = uu.invoke(s, "comm", ["file", "."])?
  uu.fails(r4)
  uu.stderr_only(r4, "comm: .: Is a directory\n")
}

# origin: uutils test_comm::test_locale_collation
test test_uu_comm_locale_collation { |ctx|
  if process.which("locale") is Err(_) {
    test.skip("en_US.UTF-8 locale is unavailable")
  }
  let locales = run.capture --text locale -a ?
  if !("en_US.utf8" in locales.stdout) and !("en_US.UTF-8" in locales.stdout) {
    test.skip("en_US.UTF-8 locale is unavailable")
  }
  let s = uu.scene(ctx)?
  uu.write(s, "f1", "a1\na-b\n")?
  uu.write(s, "f2", "a-b\n")?
  let r1 = uu.invoke(s, "comm", ["-12", "f1", "f2"], vars: {LC_ALL: "en_US.UTF-8"})?
  uu.succeeds(r1)
  uu.stdout_only(r1, "a-b\n")
  let r2 = uu.invoke(s, "comm", ["-23", "f1", "f2"], vars: {LC_ALL: "en_US.UTF-8"})?
  uu.succeeds(r2)
  uu.stdout_only(r2, "a1\n")
}

# origin: uutils test_comm::test_no_such_file
test test_uu_comm_no_such_file { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "comm", ["bogus_file_1", "bogus_file_2"])?
  uu.fails(r1)
  uu.stderr_only(r1, "comm: bogus_file_1: No such file or directory\n")
}

# origin: uutils test_comm::test_out_of_order_input_nocheck
test test_uu_comm_out_of_order_input_nocheck { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file_a", "1\n3\n")?
  uu.write(s, "file_b", "3\n2\n")?
  let r1 = uu.invoke(s, "comm", ["--nocheck-order", "file_a", "file_b"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1\n\t\t3\n\t2\n")
  uu.no_stderr(r1)
}

# origin: uutils test_comm::test_output_lossy_utf8
test test_uu_comm_output_lossy_utf8 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "a", b"\xfe\n\xff\n")?
  uu.write_bytes(s, "b", b"\xff\n\xfe\n")?
  let r1 = uu.invoke(s, "comm", ["a", "b"])?
  uu.fails(r1)
  uu.stdout_is_bytes(r1, b"\xfe\n\t\t\xff\n\t\xfe\n")
}

# origin: uutils test_comm::test_sorted
test test_uu_comm_sorted { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "comm1", "1\n3")?
  uu.write(s, "comm2", "3\n2")?
  let r1 = uu.invoke(s, "comm", ["comm1", "comm2"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_is(r1, "1\n\t\t3\n\t2\n")
  uu.stderr_is(r1, "comm: file 2 is not in sorted order\ncomm: input is not in sorted order\n")
}

# origin: uutils test_comm::test_sorted_check_order
test test_uu_comm_sorted_check_order { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "comm1", "1\n3")?
  uu.write(s, "comm2", "3\n2")?
  let r1 = uu.invoke(s, "comm", ["--check-order", "comm1", "comm2"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_is(r1, "1\n\t\t3\n")
  uu.stderr_is(r1, "comm: file 2 is not in sorted order\n")
}

# origin: uutils test_comm::total
test test_uu_comm_total { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.write(s, "b", "b\nz\n")?
  let r1 = uu.invoke(s, "comm", ["--total", "a", "b"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\n\tb\n\t\tz\n1\t1\t1\ttotal\n")
}

# origin: uutils test_comm::total_with_output_delimiter
test test_uu_comm_total_with_output_delimiter { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.write(s, "b", "b\nz\n")?
  let r1 = uu.invoke(s, "comm", ["--total", "--output-delimiter=word", "a", "b"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\nwordb\nwordwordz\n1word1word1wordtotal\n")
}

# origin: uutils test_comm::total_with_suppressed_regular_output
test test_uu_comm_total_with_suppressed_regular_output { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\nz\n")?
  uu.write(s, "b", "b\nz\n")?
  let r1 = uu.invoke(s, "comm", ["--total", "-123", "a", "b"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1\t1\t1\ttotal\n")
}

# origin: uutils test_comm::unintuitive_default_behavior_1
test test_uu_comm_unintuitive_default_behavior_1 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "defaultcheck_unintuitive_1", "m\nh\nn\no\nc\np\n")?
  uu.write(s, "defaultcheck_unintuitive_2", "m\nh\nn\no\np\n")?
  let r1 = uu.invoke(s, "comm", ["defaultcheck_unintuitive_1", "defaultcheck_unintuitive_2"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t\tm\n\t\th\n\t\tn\n\t\to\nc\n\t\tp\n")
}

# origin: uutils test_comm::zero_terminated
test test_uu_comm_zero_terminated { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a_nul", "a\0z\0")?
  uu.write(s, "b_nul", "b\0z\0")?
  for param in ["-z", "--zero-terminated"] {
    let r1 = uu.invoke(s, "comm", [param, "a_nul", "b_nul"])?
    uu.succeeds(r1)
    uu.stdout_is(r1, "a\0\tb\0\t\tz\0")
  }
}

# origin: uutils test_comm::zero_terminated_provided_multiple_times
test test_uu_comm_zero_terminated_provided_multiple_times { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a_nul", "a\0z\0")?
  uu.write(s, "b_nul", "b\0z\0")?
  for param in ["-z", "--zero-terminated"] {
    let r1 = uu.invoke(s, "comm", [param, param, param, "a_nul", "b_nul"])?
    uu.succeeds(r1)
    uu.stdout_is(r1, "a\0\tb\0\t\tz\0")
  }
}

# origin: uutils test_comm::zero_terminated_with_total
test test_uu_comm_zero_terminated_with_total { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a_nul", "a\0z\0")?
  uu.write(s, "b_nul", "b\0z\0")?
  for param in ["-z", "--zero-terminated"] {
    let r1 = uu.invoke(s, "comm", [param, "--total", "a_nul", "b_nul"])?
    uu.succeeds(r1)
    uu.stdout_is(r1, "a\0\tb\0\t\tz\01\t1\t1\ttotal\0")
  }
}
