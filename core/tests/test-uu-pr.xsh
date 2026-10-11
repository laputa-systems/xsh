##! Native ports of the uutils pr integration tests.

use support.uu as uu

# File headers use that input's actual modification time in UTC.
proc expected_file(s: uu.Scene, fixture: Str, input: Str) [fs, time, error] -> Result[Bytes, Error] {
  let pattern = fp"{s.ctx.core_dir}/tests/data/uutils/pr/{fixture}".read_text()?
  let stamp = time.format(fs.stat(uu.at(s, input))?.mtime_ns, "%Y-%m-%d %H:%M", utc: true)?
  Ok(bytes.from_text(pattern.replace("{last_modified_time}", with: stamp)))
}

# Stdin and merged headers can cross a minute while the process runs.
proc expected_during_run(s: uu.Scene, r: uu.Ran, fixture: Str, started: Int) [fs, time, error] {
  let pattern = fp"{s.ctx.core_dir}/tests/data/uutils/pr/{fixture}".read_text()?
  let end = time.now() + 60000
  var current = started
  var found = false
  while current < end {
    let stamp = time.format(current * 1000000, "%Y-%m-%d %H:%M", utc: true)?
    if r.stdout == bytes.from_text(pattern.replace("{last_modified_time}", with: stamp)) { found = true; break }
    current += 60000
  }
  assert found, "stdout differs from every fixture rendered within the run's minute interval"
}

# origin: uutils test_pr::test_without_any_options
test test_uu_pr_without_any_options { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test_one_page.log")?
  uu.fixture(s, "pr", "test_one_page.log", "test_one_page.log")?
  let r0 = uu.invoke(s, "pr", ["test_one_page.log"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, expected_file(s, "test_one_page.log.expected", "test_one_page.log")?)
}

# origin: uutils test_pr::test_with_numbering_option_with_number_width
test test_uu_pr_with_numbering_option_with_number_width { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test_num_page.log")?
  uu.fixture(s, "pr", "test_num_page.log", "test_num_page.log")?
  let r0 = uu.invoke(s, "pr", ["-n", "2", "test_num_page.log"], stdin: b"")?
  uu.fails_with_code(r0, 1)
  uu.stderr_is(r0, "pr: 2: No such file or directory\n")
  uu.stdout_is_bytes(r0, expected_file(s, "gnu-test_num_page_2.log.expected", "test_num_page.log")?)
}

# origin: uutils test_pr::test_with_double_space_option
test test_uu_pr_with_double_space_option { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test_one_page.log")?
  uu.fixture(s, "pr", "test_one_page.log", "test_one_page.log")?
  let r0 = uu.invoke(s, "pr", ["-d", "test_one_page.log"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, expected_file(s, "test_one_page_double_line.log.expected", "test_one_page.log")?)
  uu.remove(s, "test_one_page.log")?
  uu.fixture(s, "pr", "test_one_page.log", "test_one_page.log")?
  let r1 = uu.invoke(s, "pr", ["--double-space", "test_one_page.log"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, expected_file(s, "test_one_page_double_line.log.expected", "test_one_page.log")?)
}

# origin: uutils test_pr::test_with_first_line_number_option
test test_uu_pr_with_first_line_number_option { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test_one_page.log")?
  uu.fixture(s, "pr", "test_one_page.log", "test_one_page.log")?
  let r0 = uu.invoke(s, "pr", ["-N", "5", "-n", "test_one_page.log"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, expected_file(s, "test_one_page_first_line.log.expected", "test_one_page.log")?)
}

# origin: uutils test_pr::test_with_first_line_number_long_option
test test_uu_pr_with_first_line_number_long_option { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test_one_page.log")?
  uu.fixture(s, "pr", "test_one_page.log", "test_one_page.log")?
  let r0 = uu.invoke(s, "pr", ["--first-line-number=5", "-n", "test_one_page.log"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, expected_file(s, "test_one_page_first_line.log.expected", "test_one_page.log")?)
}

# origin: uutils test_pr::test_with_number_option_with_custom_separator_char
test test_uu_pr_with_number_option_with_custom_separator_char { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test_num_page.log")?
  uu.fixture(s, "pr", "test_num_page.log", "test_num_page.log")?
  let r0 = uu.invoke(s, "pr", ["-nc", "test_num_page.log"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, expected_file(s, "test_num_page_char.log.expected", "test_num_page.log")?)
}

# origin: uutils test_pr::test_with_number_option_with_custom_separator_char_and_width
test test_uu_pr_with_number_option_with_custom_separator_char_and_width { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test_num_page.log")?
  uu.fixture(s, "pr", "test_num_page.log", "test_num_page.log")?
  let r0 = uu.invoke(s, "pr", ["-nc1", "test_num_page.log"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, expected_file(s, "test_num_page_char_one.log.expected", "test_num_page.log")?)
}

# origin: uutils test_pr::test_with_page_range
test test_uu_pr_with_page_range { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test.log")?
  uu.fixture(s, "pr", "test.log", "test.log")?
  let r0 = uu.invoke(s, "pr", ["--pages=15", "test.log"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, expected_file(s, "test_page_range_1.log.expected", "test.log")?)
  uu.remove(s, "test.log")?
  uu.fixture(s, "pr", "test.log", "test.log")?
  let r1 = uu.invoke(s, "pr", ["+15", "test.log"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, expected_file(s, "test_page_range_1.log.expected", "test.log")?)
  uu.remove(s, "test.log")?
  uu.fixture(s, "pr", "test.log", "test.log")?
  let r2 = uu.invoke(s, "pr", ["--pages=15:17", "test.log"], stdin: b"")?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, expected_file(s, "test_page_range_2.log.expected", "test.log")?)
  uu.remove(s, "test.log")?
  uu.fixture(s, "pr", "test.log", "test.log")?
  let r3 = uu.invoke(s, "pr", ["+15:17", "test.log"], stdin: b"")?
  uu.succeeds(r3)
  uu.stdout_is_bytes(r3, expected_file(s, "test_page_range_2.log.expected", "test.log")?)
}

# origin: uutils test_pr::test_with_no_header_trailer_option
test test_uu_pr_with_no_header_trailer_option { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test_one_page.log")?
  uu.fixture(s, "pr", "test_one_page.log", "test_one_page.log")?
  let r0 = uu.invoke(s, "pr", ["-t", "test_one_page.log"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, expected_file(s, "test_one_page_no_ht.log.expected", "test_one_page.log")?)
}

# origin: uutils test_pr::test_with_page_length_option
test test_uu_pr_with_page_length_option { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test.log")?
  uu.fixture(s, "pr", "test.log", "test.log")?
  let r0 = uu.invoke(s, "pr", ["--pages=2:3", "-l", "100", "-n", "test.log"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, expected_file(s, "test_page_length.log.expected", "test.log")?)
  uu.remove(s, "test.log")?
  uu.fixture(s, "pr", "test.log", "test.log")?
  let r1 = uu.invoke(s, "pr", ["--pages=2:3", "-l", "5", "-n", "test.log"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, expected_file(s, "test_page_length1.log.expected", "test.log")?)
}

# origin: uutils test_pr::test_with_stdin
test test_uu_pr_with_stdin { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "stdin.log")?
  uu.fixture(s, "pr", "stdin.log", "stdin.log")?
  let started0 = time.now()
  let r0 = uu.invoke(s, "pr", ["--pages=1:2", "-n", "-"], stdin: uu.read(s, "stdin.log")?)?
  uu.succeeds(r0)
  expected_during_run(s, r0, "stdin.log.expected", started0)
}

# origin: uutils test_pr::test_with_columns
test test_uu_pr_with_columns { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "column.log")?
  uu.fixture(s, "pr", "column.log", "column.log")?
  let r0 = uu.invoke(s, "pr", ["--pages=3:5", "-3", "-n", "column.log"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, expected_file(s, "gnu-column.log.expected", "column.log")?)
  uu.remove(s, "column.log")?
  uu.fixture(s, "pr", "column.log", "column.log")?
  let r1 = uu.invoke(s, "pr", ["--pages=3:5", "--columns=3", "-n", "column.log"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, expected_file(s, "gnu-column.log.expected", "column.log")?)
}

# origin: uutils test_pr::test_with_columns_and_across_option
test test_uu_pr_with_columns_and_across_option { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "column.log")?
  uu.fixture(s, "pr", "column.log", "column.log")?
  let r0 = uu.invoke(s, "pr", ["--pages=3:5", "--columns=3", "-a", "-n", "column.log"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, expected_file(s, "gnu-column_across.log.expected", "column.log")?)
}

# origin: uutils test_pr::test_with_columns_across_option_and_column_separator
test test_uu_pr_with_columns_across_option_and_column_separator { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "column.log")?
  uu.fixture(s, "pr", "column.log", "column.log")?
  let r0 = uu.invoke(s, "pr", ["--pages=3:5", "--columns=3", "-s|", "-a", "-n", "column.log"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, expected_file(s, "gnu-column_across_sep.log.expected", "column.log")?)
  uu.remove(s, "column.log")?
  uu.fixture(s, "pr", "column.log", "column.log")?
  let r1 = uu.invoke(s, "pr", ["--pages=3:5", "--columns=3", "-Sdivide", "-a", "-n", "column.log"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, expected_file(s, "gnu-column_across_sep1.log.expected", "column.log")?)
}

# origin: uutils test_pr::test_with_mpr
test test_uu_pr_with_mpr { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "column.log")?
  uu.fixture(s, "pr", "column.log", "column.log")?
  uu.remove(s, "hosts.log")?
  uu.fixture(s, "pr", "hosts.log", "hosts.log")?
  let started0 = time.now()
  let r0 = uu.invoke(s, "pr", ["--pages=1:2", "-m", "-n", "column.log", "hosts.log"], stdin: b"")?
  uu.succeeds(r0)
  expected_during_run(s, r0, "gnu-mpr.log.expected", started0)
  uu.remove(s, "column.log")?
  uu.fixture(s, "pr", "column.log", "column.log")?
  uu.remove(s, "hosts.log")?
  uu.fixture(s, "pr", "hosts.log", "hosts.log")?
  let started1 = time.now()
  let r1 = uu.invoke(s, "pr", ["--pages=2:4", "-m", "-n", "column.log", "hosts.log"], stdin: b"")?
  uu.succeeds(r1)
  expected_during_run(s, r1, "gnu-mpr1.log.expected", started1)
  uu.remove(s, "column.log")?
  uu.fixture(s, "pr", "column.log", "column.log")?
  uu.remove(s, "hosts.log")?
  uu.fixture(s, "pr", "hosts.log", "hosts.log")?
  let started2 = time.now()
  let r2 = uu.invoke(s, "pr", ["--pages=1:2", "-l", "100", "-n", "-m", "column.log", "hosts.log", "column.log"], stdin: b"")?
  uu.succeeds(r2)
  expected_during_run(s, r2, "gnu-mpr2.log.expected", started2)
}

# origin: uutils test_pr::test_with_offset_space_option
test test_uu_pr_with_offset_space_option { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "column.log")?
  uu.fixture(s, "pr", "column.log", "column.log")?
  let r0 = uu.invoke(s, "pr", ["-o", "5", "--pages=3:5", "--columns=3", "-a", "-n", "column.log"], stdin: b"")?
  uu.succeeds(r0)
  uu.stdout_is_bytes(r0, expected_file(s, "gnu-column_spaces_across.log.expected", "column.log")?)
}

# origin: uutils test_pr::test_with_join_lines_option
test test_uu_pr_with_join_lines_option { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "hosts.log")?
  uu.fixture(s, "pr", "hosts.log", "hosts.log")?
  uu.remove(s, "test.log")?
  uu.fixture(s, "pr", "test.log", "test.log")?
  let started0 = time.now()
  let r0 = uu.invoke(s, "pr", ["+1:2", "-J", "-m", "hosts.log", "test.log"], stdin: b"")?
  uu.succeeds(r0)
  expected_during_run(s, r0, "gnu-joined.log.expected", started0)
  uu.remove(s, "hosts.log")?
  uu.fixture(s, "pr", "hosts.log", "hosts.log")?
  uu.remove(s, "test.log")?
  uu.fixture(s, "pr", "test.log", "test.log")?
  let started1 = time.now()
  let r1 = uu.invoke(s, "pr", ["+1:2", "--join-lines", "-m", "hosts.log", "test.log"], stdin: b"")?
  uu.succeeds(r1)
  expected_during_run(s, r1, "gnu-joined.log.expected", started1)
}

# origin: uutils test_pr::test_invalid_flag
test test_uu_pr_invalid_flag { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["--invalid-argument"], stdin: bytes.from_text(""))?
  uu.fails_with_code(r0, 1)
  uu.no_stdout(r0)
}

# origin: uutils test_pr::test_number_lines_empty_value_is_rejected
test test_uu_pr_number_lines_empty_value_is_rejected { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test_one_page.log")?
  uu.fixture(s, "pr", "test_one_page.log", "test_one_page.log")?
  let r0 = uu.invoke(s, "pr", ["--number-lines=", "test_one_page.log"], stdin: bytes.from_text(""))?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "pr: '-n': Invalid argument: ''")
}

# origin: uutils test_pr::test_start_page_exceeds_page_count
test test_uu_pr_start_page_exceeds_page_count { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "hosts.log")?
  uu.fixture(s, "pr", "hosts.log", "hosts.log")?
  let r0 = uu.invoke(s, "pr", ["--pages=2", "hosts.log"], stdin: bytes.from_text(""))?
  uu.succeeds(r0)
  uu.stderr_only(r0, "pr: starting page number 2 exceeds page count 1\n")
}

# origin: uutils test_pr::test_with_suppress_error_option
test test_uu_pr_with_suppress_error_option { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test_num_page.log")?
  uu.fixture(s, "pr", "test_num_page.log", "test_num_page.log")?
  let r0 = uu.invoke(s, "pr", ["--pages=20:5", "-r", "test_num_page.log"], stdin: bytes.from_text(""))?
  uu.fails(r0)
  uu.no_stdout(r0)
  uu.stderr_is(r0, "pr: invalid page range '20:5'\n")
}

# origin: uutils test_pr::test_help
test test_uu_pr_help { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["--help"], stdin: bytes.from_text(""))?
  uu.succeeds(r0)
}

# origin: uutils test_pr::test_version
test test_uu_pr_version { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["--version"], stdin: bytes.from_text(""))?
  uu.succeeds(r0)
}

# origin: uutils test_pr::test_pr_char_device_dev_null
test test_uu_pr_pr_char_device_dev_null { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["/dev/null"], stdin: bytes.from_text(""))?
  uu.succeeds(r0)
}

# origin: uutils test_pr::test_b_flag_backwards_compat
test test_uu_pr_b_flag_backwards_compat { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-b", "-t"], stdin: bytes.from_text("a\nb\n"))?
  uu.succeeds(r0)
}

# origin: uutils test_pr::test_separator_options_default_values
test test_uu_pr_separator_options_default_values { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-t", "-2", "-s"], stdin: bytes.from_text("a\nb\n"))?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "pr", ["-t", "-2", "-S"], stdin: bytes.from_text("a\nb\n"))?
  uu.succeeds(r1)
}

# origin: uutils test_pr::test_omit_pagination_option
test test_uu_pr_omit_pagination_option { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-T"], stdin: bytes.from_text("a\nb\n"))?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "pr", ["--omit-pagination"], stdin: bytes.from_text("a\nb\n"))?
  uu.succeeds(r1)
}

# origin: uutils test_pr::test_large_page_width_does_not_panic
test test_uu_pr_large_page_width_does_not_panic { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-W", "200000"], stdin: bytes.from_text("x\n"))?
  uu.succeeds(r0)
}

# origin: uutils test_pr::test_expand_tab_does_not_consume_following_operand
test test_uu_pr_expand_tab_does_not_consume_following_operand { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-t", "-e"], stdin: bytes.from_text("a\tb\n"))?
  uu.succeeds(r0)
  uu.stdout_only(r0, "a       b\n")
}

# origin: uutils test_pr::test_columns_partly_filled_page
test test_uu_pr_columns_partly_filled_page { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-t", "-2", "-w", "20"], stdin: bytes.from_text("a\nb\nc\n"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "a\t  c\nb\n")
}

# origin: uutils test_pr::test_columns_partly_filled_page_across
test test_uu_pr_columns_partly_filled_page_across { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-t", "-a", "-2", "-w", "20"], stdin: bytes.from_text("a\nb\nc\n"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "a\t  b\nc\n")
}

# origin: uutils test_pr::test_columns_fewer_lines_than_columns
test test_uu_pr_columns_fewer_lines_than_columns { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-t", "-3", "-w", "20"], stdin: bytes.from_text("a\n"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "a\n")
  let r1 = uu.invoke(s, "pr", ["-t", "-3", "-w", "20"], stdin: bytes.from_text("a\nb\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a      b\n")
}

# origin: uutils test_pr::test_number_lines_without_value_numbers_lines
test test_uu_pr_number_lines_without_value_numbers_lines { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-t", "-n"], stdin: bytes.from_text("a\nb\n"))?
  uu.succeeds(r0)
  uu.stdout_is(r0, "    1\ta\n    2\tb\n")
  let r1 = uu.invoke(s, "pr", ["-t", "--number-lines"], stdin: bytes.from_text("a\nb\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "    1\ta\n    2\tb\n")
}

# origin: uutils test_pr::test_expand_tab_at_end_of_short_flag_cluster
test test_uu_pr_expand_tab_at_end_of_short_flag_cluster { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-tre"], stdin: bytes.from_text("oi\n"))?
  uu.succeeds(r0)
  uu.stdout_only(r0, "oi\n")
  let r1 = uu.invoke(s, "pr", ["-tre8"], stdin: bytes.from_text("oi\n"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "oi\n")
  let r2 = uu.invoke(s, "pr", ["-tfre"], stdin: bytes.from_text("oi\n"))?
  uu.succeeds(r2)
  uu.stdout_only(r2, "oi\n")
  let r3 = uu.invoke(s, "pr", ["-tfre8"], stdin: bytes.from_text("oi\n"))?
  uu.succeeds(r3)
  uu.stdout_only(r3, "oi\n")
}

# origin: uutils test_pr::test_page_length_ten_implies_omit_header
test test_uu_pr_page_length_ten_implies_omit_header { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-l", "10", "-h", "hdr"], stdin: bytes.from_text("a\nb\nc\n"))?
  uu.succeeds(r0)
  uu.stdout_only(r0, "a\nb\nc\n")
  let r1 = uu.invoke(s, "pr", ["-l", "10", "-t"], stdin: bytes.from_text("a\nb\nc\n"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "a\nb\nc\n")
}

# origin: uutils test_pr::test_page_length_eleven_keeps_header
test test_uu_pr_page_length_eleven_keeps_header { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-l", "11", "-h", "hdr"], stdin: bytes.from_text("a\nb\nc\n"))?
  uu.succeeds(r0)
  uu.stdout_contains(r0, "hdr")
}

# origin: uutils test_pr::test_merge_empty_input
test test_uu_pr_merge_empty_input { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-m", "/dev/null", "/dev/null"], stdin: bytes.from_text(""))?
  uu.succeeds(r0)
  uu.no_output(r0)
}

# origin: uutils test_pr::test_missing_file_error_message
test test_uu_pr_missing_file_error_message { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["nonexistent_file"], stdin: bytes.from_text(""))?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "pr: nonexistent_file: ")
  assert ! ("(os error" in r0.stderr.utf8()?)
}

# origin: uutils test_pr::test_with_mpr_and_columns_options
test test_uu_pr_with_mpr_and_columns_options { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "column.log")?
  uu.fixture(s, "pr", "column.log", "column.log")?
  let r0 = uu.invoke(s, "pr", ["--columns=2", "-m", "-n", "column.log"], stdin: bytes.from_text(""))?
  uu.fails(r0)
  uu.stderr_only(r0, "pr: cannot specify number of columns when printing in parallel\n")
  uu.remove(s, "column.log")?
  uu.fixture(s, "pr", "column.log", "column.log")?
  let r1 = uu.invoke(s, "pr", ["-a", "-m", "-n", "column.log"], stdin: bytes.from_text(""))?
  uu.fails(r1)
  uu.stderr_only(r1, "pr: cannot specify both printing across and printing in parallel\n")
}

# origin: uutils test_pr::test_value_for_number_lines
test test_uu_pr_value_for_number_lines { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test.log")?
  uu.fixture(s, "pr", "test.log", "test.log")?
  let r0 = uu.invoke(s, "pr", ["-n", "*5", "test.log"], stdin: bytes.from_text(""))?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "pr: '*5': No such file or directory")
  uu.remove(s, "test.log")?
  uu.fixture(s, "pr", "test.log", "test.log")?
  let r1 = uu.invoke(s, "pr", ["-n", "a", "test.log"], stdin: bytes.from_text(""))?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "pr: a: No such file or directory")
  uu.remove(s, "test.log")?
  uu.fixture(s, "pr", "test.log", "test.log")?
  let r2 = uu.invoke(s, "pr", ["-n", "foo5.txt", "test.log"], stdin: bytes.from_text(""))?
  uu.fails(r2)
}

# origin: uutils test_pr::test_expand_tab_does_not_consume_next_argument
test test_uu_pr_expand_tab_does_not_consume_next_argument { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "empty_test_file")?
  uu.fixture(s, "pr", "empty_test_file", "empty_test_file")?
  let r0 = uu.invoke(s, "pr", ["-e", "empty_test_file"], stdin: bytes.from_text(""))?
  uu.succeeds(r0)
  uu.remove(s, "empty_test_file")?
  uu.fixture(s, "pr", "empty_test_file", "empty_test_file")?
  let r1 = uu.invoke(s, "pr", ["-ea", "empty_test_file"], stdin: bytes.from_text(""))?
  uu.succeeds(r1)
  uu.remove(s, "empty_test_file")?
  uu.fixture(s, "pr", "empty_test_file", "empty_test_file")?
  let r2 = uu.invoke(s, "pr", ["-ea1", "empty_test_file"], stdin: bytes.from_text(""))?
  uu.succeeds(r2)
}

# origin: uutils test_pr::test_offset_invalid
test test_uu_pr_offset_invalid { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["--indent=-5"], stdin: bytes.from_text(""))?
  uu.fails_with_code(r0, 1)
  uu.stderr_is(r0, "pr: '-o MARGIN' invalid line offset: '-5': Value too large for defined data type\n")
  let r1 = uu.invoke(s, "pr", ["-o", "abc"], stdin: bytes.from_text(""))?
  uu.fails_with_code(r1, 1)
  uu.stderr_is(r1, "pr: '-o MARGIN' invalid line offset: 'abc'\n")
}

# origin: uutils test_pr::test_expand_tabs_multibyte_char_is_rejected
test test_uu_pr_expand_tabs_multibyte_char_is_rejected { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test_one_page.log")?
  uu.fixture(s, "pr", "test_one_page.log", "test_one_page.log")?
  for arg in ["-e€", "-e€3"] {
    let r = uu.invoke(s, "pr", [arg, "test_one_page.log"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "pr: '-e' extra characters or invalid number in the argument")
  }
}

# origin: uutils test_pr::test_number_lines_multibyte_separator_is_rejected
test test_uu_pr_number_lines_multibyte_separator_is_rejected { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test_one_page.log")?
  uu.fixture(s, "pr", "test_one_page.log", "test_one_page.log")?
  for arg in ["-n€", "-n€5"] {
    let r = uu.invoke(s, "pr", [arg, "test_one_page.log"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, "pr: '-n' extra characters or invalid number in the argument")
  }
}

# origin: uutils test_pr::test_offset_too_large
test test_uu_pr_offset_too_large { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pr", ["-o", "2147483648"], stdin: b"")?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "pr: '-o MARGIN' invalid line offset: '2147483648': Value too large for defined data type\n")
}

# origin: uutils test_pr::test_start_line_number_too_large
test test_uu_pr_start_line_number_too_large { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pr", ["-n", "-N", "18446744073709551615"], stdin: b"")?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "pr: '-N NUMBER' invalid starting line number: '18446744073709551615': Value too large for defined data type\n")
}

# origin: uutils test_pr::test_page_length_too_large
test test_uu_pr_page_length_too_large { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pr", ["-l", "9999999999999999999", "-3"], stdin: b"")?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "pr: '-l PAGE_LENGTH' invalid number of lines: '9999999999999999999': Value too large for defined data type\n")
}

# origin: uutils test_pr::test_page_width_too_large
test test_uu_pr_page_width_too_large { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pr", ["-W", "18446744073709551615"], stdin: b"")?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "pr: '-W PAGE_WIDTH' invalid number of characters: '18446744073709551615': Value too large for defined data type\n")
}

# origin: uutils test_pr::test_column_width_too_large
test test_uu_pr_column_width_too_large { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pr", ["-w", "18446744073709551615", "-2"], stdin: b"")?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "pr: '-w PAGE_WIDTH' invalid number of characters: '18446744073709551615': Value too large for defined data type\n")
}

# origin: uutils test_pr::test_column_count_too_large
test test_uu_pr_column_count_too_large { |ctx|
  let s = uu.scene(ctx)?
  let long = uu.invoke(s, "pr", ["--columns", "9999999999999999999"], stdin: b"")?
  uu.fails_with_code(long, 1)
  uu.stderr_is(long, "pr: invalid number of columns: '9999999999999999999': Value too large for defined data type\n")
  let legacy = uu.invoke(s, "pr", ["-9999999999999999999"], stdin: b"")?
  uu.fails_with_code(legacy, 1)
  uu.stderr_is(legacy, "pr: invalid number of columns: '9999999999999999999': Value too large for defined data type\n")
}

# origin: uutils test_pr::test_large_number_width_does_not_panic
test test_uu_pr_large_number_width_does_not_panic { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pr", ["-t", "-n", "70000"], stdin: b"x\n")?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_is(r, "pr: 70000: No such file or directory\n")
}

# origin: uutils test_pr::test_filename_ending_with_dash_number_is_not_an_option
test test_uu_pr_filename_ending_with_dash_number_is_not_an_option { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a-0", "a-b-0", "a-3"] {
    uu.write(s, name, "RUST-pr\n")?
    let r = uu.invoke(s, "pr", ["-t", name])?
    uu.succeeds(r)
    uu.stdout_contains(r, "RUST-pr")
  }
}

# origin: uutils test_pr::test_double_dash_shields_filename_ending_with_dash_zero
test test_uu_pr_double_dash_shields_filename_ending_with_dash_zero { |ctx|
  let s = uu.scene(ctx)?
  for name in ["a-0", "a-b-0"] {
    uu.write(s, name, "RUST-pr\n")?
    let r = uu.invoke(s, "pr", ["-t", "--", name])?
    uu.succeeds(r)
    uu.stdout_contains(r, "RUST-pr\n")
  }
}

# origin: uutils test_pr::test_double_dash_terminates_option_parsing
test test_uu_pr_double_dash_terminates_option_parsing { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "-0", "RUST-pr\n")?
  let r = uu.invoke(s, "pr", ["-t", "--", "-0"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_contains(r, "RUST-pr")
}

# origin: uutils test_pr::test_double_dash_shields_expand_tabs_filename
test test_uu_pr_double_dash_shields_expand_tabs_filename { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "-e", "RUST-pr\n")?
  let r = uu.invoke(s, "pr", ["-t", "--", "-e"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_contains(r, "RUST-pr")
}

# origin: uutils test_pr::test_double_dash_shields_number_lines_filename
test test_uu_pr_double_dash_shields_number_lines_filename { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "-n", "first\n")?
  uu.write(s, "data", "second\n")?
  let r = uu.invoke(s, "pr", ["-t", "--", "-n", "data"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_contains(r, "first")
  uu.stdout_contains(r, "second")
}

# origin: uutils test_pr::test_with_long_header_option
test test_uu_pr_with_long_header_option { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-h", "new file"], stdin: bytes.from_text("a"))?
  uu.succeeds(r0)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                     new file                     Page 1\n\n\na\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n".matches(r0.stdout.utf8()?)
  let r1 = uu.invoke(s, "pr", ["--header=new file"], stdin: bytes.from_text("a"))?
  uu.succeeds(r1)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                     new file                     Page 1\n\n\na\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n".matches(r1.stdout.utf8()?)
}

# origin: uutils test_pr::test_page_header_width
test test_uu_pr_page_header_width { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", [], stdin: bytes.from_text("a"))?
  uu.succeeds(r0)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\na\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n".matches(r0.stdout.utf8()?)
}

# origin: uutils test_pr::test_form_feed_newlines
test test_uu_pr_form_feed_newlines { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-f"], stdin: bytes.from_text("\x0c\x0c"))?
  uu.succeeds(r0)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\n\n\x0c\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 2\n\n\n\n\x0c".matches(r0.stdout.utf8()?)
}

# origin: uutils test_pr::test_new_line_followed_by_form_feed
test test_uu_pr_new_line_followed_by_form_feed { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-f"], stdin: bytes.from_text("abc\n\x0c"))?
  uu.succeeds(r0)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\nabc\n\x0c".matches(r0.stdout.utf8()?)
}

# origin: uutils test_pr::test_form_feed_followed_by_new_line
test test_uu_pr_form_feed_followed_by_new_line { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", [], stdin: bytes.from_text("\x0c\nabc"))?
  uu.succeeds(r0)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 2\n\n\nabc\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n".matches(r0.stdout.utf8()?)
}

# origin: uutils test_pr::test_columns
test test_uu_pr_columns { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-2"], stdin: bytes.from_text("a\nb\n"))?
  uu.succeeds(r0)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\na\t\t\t\t    b\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n".matches(r0.stdout.utf8()?)
}

# origin: uutils test_pr::test_merge
test test_uu_pr_merge { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f", "a\n")?
  uu.write(s, "g", "b\n")?
  let r0 = uu.invoke(s, "pr", ["-m", "f", "g"], stdin: bytes.from_text(""))?
  uu.succeeds(r0)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\na\t\t\t\t    b\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n".matches(r0.stdout.utf8()?)
}

# origin: uutils test_pr::test_merge_one_long_one_short
test test_uu_pr_merge_one_long_one_short { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f", "a\na\n")?
  uu.write(s, "g", "b\n")?
  let r0 = uu.invoke(s, "pr", ["-l", "11", "-m", "f", "g"], stdin: bytes.from_text(""))?
  uu.succeeds(r0)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\na\t\t\t\t    b\n\n\n\n\n\n\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 2\n\n\na\t\t\t\t    \n\n\n\n\n\n".matches(r0.stdout.utf8()?)
}

# origin: uutils test_pr::test_simple_expand_tab
test test_uu_pr_simple_expand_tab { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-e"], stdin: bytes.from_text("hello\tworld\nabc\tdef\n\tleading\ntrail\t\n8chars00\t\n"))?
  uu.succeeds(r0)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\nhello   world\nabc     def\n        leading\ntrail   \n8chars00        \n".matches(r0.stdout.utf8()?)
}

# origin: uutils test_pr::test_simple_expand_tab_with_digit_argument
test test_uu_pr_simple_expand_tab_with_digit_argument { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-e2"], stdin: bytes.from_text("hello\tworld\nabc\tdef\n\tleading\ntrail\t\n8chars00\t\n"))?
  uu.succeeds(r0)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\nhello world\nabc def\n  leading\ntrail \n8chars00  \n".matches(r0.stdout.utf8()?)
  let r1 = uu.invoke(s, "pr", ["-e3"], stdin: bytes.from_text("hello\tworld\nabc\tdef\n\tleading\ntrail\t\n8chars00\t\n"))?
  uu.succeeds(r1)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\nhello world\nabc   def\n   leading\ntrail \n8chars00 \n".matches(r1.stdout.utf8()?)
  let r2 = uu.invoke(s, "pr", ["-e8"], stdin: bytes.from_text("hello\tworld\nabc\tdef\n\tleading\ntrail\t\n8chars00\t\n"))?
  uu.succeeds(r2)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\nhello   world\nabc     def\n        leading\ntrail   \n8chars00        \n".matches(r2.stdout.utf8()?)
  let r3 = uu.invoke(s, "pr", ["-e10"], stdin: bytes.from_text("hello\tworld\nabc\tdef\n\tleading\ntrail\t\n8chars00\t\n"))?
  uu.succeeds(r3)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\nhello     world\nabc       def\n          leading\ntrail     \n8chars00  \n".matches(r3.stdout.utf8()?)
}

# origin: uutils test_pr::test_simple_expand_tab_with_char_argument
test test_uu_pr_simple_expand_tab_with_char_argument { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-ea"], stdin: bytes.from_text("hello\tworld\nabc\tdef\n\tleading\ntrail\t\n8chars00\t\n"))?
  uu.succeeds(r0)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\nhello   world\n        bc      def\n        le      ding\ntr      il      \n8ch     rs00    \n".matches(r0.stdout.utf8()?)
  let r1 = uu.invoke(s, "pr", ["-ee"], stdin: bytes.from_text("hello\tworld\nabc\tdef\n\tleading\ntrail\t\n8chars00\t\n"))?
  uu.succeeds(r1)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\nh       llo     world\nabc     d       f\n        l       ading\ntrail   \n8chars00        \n".matches(r1.stdout.utf8()?)
}

# origin: uutils test_pr::test_simple_expand_tab_with_both_arguments
test test_uu_pr_simple_expand_tab_with_both_arguments { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", ["-ea2"], stdin: bytes.from_text("hello\tworld\nabc\tdef\n\tleading\ntrail\t\n8chars00\t\n"))?
  uu.succeeds(r0)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\nhello   world\n  bc    def\n        le  ding\ntr  il  \n8ch rs00        \n".matches(r0.stdout.utf8()?)
  let r1 = uu.invoke(s, "pr", ["-ee3"], stdin: bytes.from_text("hello\tworld\nabc\tdef\n\tleading\ntrail\t\n8chars00\t\n"))?
  uu.succeeds(r1)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\nh  llo  world\nabc     d   f\n        l   ading\ntrail   \n8chars00        \n".matches(r1.stdout.utf8()?)
  let r2 = uu.invoke(s, "pr", ["-et10"], stdin: bytes.from_text("hello\tworld\nabc\tdef\n\tleading\ntrail\t\n8chars00\t\n"))?
  uu.succeeds(r2)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\nhello   world\nabc     def\n        leading\n          rail  \n8chars00        \n".matches(r2.stdout.utf8()?)
}

# origin: uutils test_pr::test_with_date_format
test test_uu_pr_with_date_format { |ctx|
  let s = uu.scene(ctx)?
  let formatted = uu.invoke(s, "pr", ["-D", "%Y__%s"], stdin: b"a")?
  uu.succeeds(formatted)
  assert rx"\n\n[0-9]{4}__[0-9]{10}                                                  Page 1\n\n\na\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n".matches(formatted.stdout.utf8()?)
  let literal0 = uu.invoke(s, "pr", ["-D", "Hello!"], stdin: b"a")?
  uu.succeeds(literal0)
  uu.stdout_only(literal0, "\n\nHello!                                                            Page 1\n\n\na\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n")
  let literal1 = uu.invoke(s, "pr", ["--date-format=Hello!"], stdin: b"a")?
  uu.succeeds(literal1)
  uu.stdout_only(literal1, "\n\nHello!                                                            Page 1\n\n\na\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n")
  let literal2 = uu.invoke(s, "pr", ["--date-format=Hello!"], stdin: b"a", vars: {POSIXLY_CORRECT: "1", LC_TIME: "POSIX"})?
  uu.succeeds(literal2)
  uu.stdout_only(literal2, "\n\nHello!                                                            Page 1\n\n\na\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n")
}

# origin: uutils test_pr::test_with_date_format_env
test test_uu_pr_with_date_format_env { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "pr", [], stdin: bytes.from_text("a"), vars: {POSIXLY_CORRECT: "1", LC_ALL: "POSIX"})?
  uu.succeeds(r0)
  assert rx"\n\n[A-Z][a-z][a-z] [ 0-9][0-9] [0-9][0-9]:[0-9][0-9] [0-9]{4}                                                 Page 1\n\n\na\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n".matches(r0.stdout.utf8()?)
  let r1 = uu.invoke(s, "pr", [], stdin: bytes.from_text("a"), vars: {POSIXLY_CORRECT: "1", LC_TIME: "POSIX"})?
  uu.succeeds(r1)
  assert rx"\n\n[A-Z][a-z][a-z] [ 0-9][0-9] [0-9][0-9]:[0-9][0-9] [0-9]{4}                                                 Page 1\n\n\na\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n".matches(r1.stdout.utf8()?)
  let r2 = uu.invoke(s, "pr", [], stdin: bytes.from_text("a"), vars: {LC_TIME: "POSIX"})?
  uu.succeeds(r2)
  assert rx"\n\n[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]                                                  Page 1\n\n\na\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n".matches(r2.stdout.utf8()?)
  let r3 = uu.invoke(s, "pr", [], stdin: bytes.from_text("a"), vars: {POSIXLY_CORRECT: "1", LC_TIME: "C"})?
  uu.succeeds(r3)
  assert rx"\n\n[A-Z][a-z][a-z] [ 0-9][0-9] [0-9][0-9]:[0-9][0-9] [0-9]{4}                                                 Page 1\n\n\na\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n".matches(r3.stdout.utf8()?)
}

# origin: uutils test_pr::test_columns_last_page
test test_uu_pr_columns_last_page { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pr", ["-2", "-l", "12", "-w", "20", "-D", "DATE"], stdin: b"1\n2\n3\n4\n5\n6\n7\n8\n9\n")?
  uu.succeeds(r)
  uu.stdout_is(r, "\n\nDATE          Page 1\n\n\n1\t  3\n2\t  4\n\n\n\n\n\n\n\nDATE          Page 2\n\n\n5\t  7\n6\t  8\n\n\n\n\n\n\n\nDATE          Page 3\n\n\n9\n\n\n\n\n\n\n")
}

# origin: uutils test_pr::test_header_formatting_with_custom_date_format
test test_uu_pr_header_formatting_with_custom_date_format { |ctx|
  let s = uu.scene(ctx)?
  uu.remove(s, "test_one_page.log")?
  uu.fixture(s, "pr", "test_one_page.log", "test_one_page.log")?
  let r = uu.invoke(s, "pr", ["-D", "+%Y-%m-%d %H:%M:%S %z (%Z)", "test_one_page.log"], stdin: b"")?
  uu.succeeds(r)
  let lines = r.stdout.utf8()?.lines().collect()
  assert lines.len() >= 5
  let header = lines[2]
  assert header.count_chars() == 72
  assert "test_one_page.log" in header
  assert "Page 1" in header
  let filename_pos = header.find("test_one_page.log") ?? -1
  let page_pos = header.find("Page 1") ?? -1
  assert filename_pos > 24 and filename_pos < 48
  assert page_pos >= 60
}

# origin: uutils test_pr::test_offset_large_value_does_not_abort_under_memory_limit
test test_uu_pr_offset_large_value_does_not_abort_under_memory_limit { |ctx|
  let s = uu.scene(ctx)?
  let words = [p"sh", p"-c", p"ulimit -v 204800; exec \"$@\"", p"sh"].extend(uu.argv(s, "pr", [p"-t", p"-o", p"999999999"])?)
  let status = process.run(process.command_argv("sh", words, s.root, {}, b"hi\n", p"/dev/null", uu.at(s, "stderr"), timeout: 30s))?
  assert status.exited_with(0)
}

# Optional numbering widths attach to -n; this exercises the full large field.
test test_pr_attached_large_number_width { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pr", ["-t", "-n70000"], stdin: b"x\n")?
  uu.succeeds(r)
  uu.stdout_is(r, [" " for _ in range(69999)].join("") + "1\tx\n")
}
