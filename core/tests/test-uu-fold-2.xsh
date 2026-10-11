##! Transcribed from the uutils fold integration tests.
##! Unicode width assertions require UTF-8 decoding, so those runs select C.UTF-8.

use support.uu as uu

pure repeated(value: Str, count: Int) -> Str {
  [value for _ in range(count)].join("")
}

pure character_folds(value: Str, count: Int) -> Bytes {
  bytes.from_text(repeated(f"{repeated(value, 80)}\n", count / 80) + repeated(value, count % 80))
}

# origin: uutils test_fold::test_should_preserve_final_newline_when_line_less_than_fold
test test_uu_fold_should_preserve_final_newline_when_line_less_than_fold { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fold", [], stdin: bytes.from_text("1234\n"))?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("1234\n"))
}

# origin: uutils test_fold::test_should_preserve_final_newline_when_line_longer_than_fold
test test_uu_fold_should_preserve_final_newline_when_line_longer_than_fold { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fold", ["-w2"], stdin: bytes.from_text("1234\n"))?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("12\n34\n"))
}

# origin: uutils test_fold::test_single_tab_should_not_add_extra_newline
test test_uu_fold_single_tab_should_not_add_extra_newline { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fold", ["-w1"], stdin: bytes.from_text("\t"))?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("\t"))
}

# origin: uutils test_fold::test_tab_counts_as_one_byte
test test_uu_fold_tab_counts_as_one_byte { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fold", ["-w2", "-b"], stdin: bytes.from_text("1\t2\n"))?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("1\t\n2\n"))
}

# origin: uutils test_fold::test_tab_advances_at_non_utf8_character
test test_uu_fold_tab_advances_at_non_utf8_character { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fold", "non_utf8_tab_stops.input", "non_utf8_tab_stops.input")?
  let expected = fp"{ctx.core_dir}/tests/data/uutils/fold/non_utf8_tab_stops_w8.expected".read_bytes()?
  let r = uu.invoke(s, "fold", ["-w8", "non_utf8_tab_stops.input"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected)
}

# origin: uutils test_fold::test_tab_should_advance_to_next_tab_stop
test test_uu_fold_tab_should_advance_to_next_tab_stop { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fold", "tab_stops.input", "tab_stops.input")?
  let expected = fp"{ctx.core_dir}/tests/data/uutils/fold/tab_stops_w8.expected".read_bytes()?
  let r = uu.invoke(s, "fold", ["-w8", "tab_stops.input"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected)
}

# origin: uutils test_fold::test_wide_characters_in_column_mode
test test_uu_fold_wide_characters_in_column_mode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fold", ["-w", "5"], stdin: bytes.from_text("뉐뉐뉐\n"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("뉐뉐\n뉐\n"))
}

# origin: uutils test_fold::test_wide_characters_with_characters_option
test test_uu_fold_wide_characters_with_characters_option { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fold", ["--characters", "-w", "5"], stdin: bytes.from_text("뉐뉐뉐\n"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("뉐뉐뉐\n"))
}

# origin: uutils test_fold::test_wide_characters_with_characters_short_option
test test_uu_fold_wide_characters_with_characters_short_option { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fold", ["-c", "-w", "5"], stdin: bytes.from_text("뉐뉐뉐\n"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("뉐뉐뉐\n"))
}

# origin: uutils test_fold::test_unicode_on_reader_buffer_boundary_in_character_mode
test test_uu_fold_unicode_on_reader_buffer_boundary_in_character_mode { |ctx|
  let s = uu.scene(ctx)?
  # The upstream reader capacity is 8192 bytes on this Linux target.
  let input = repeated("a", 8191) + "뉐" + repeated("a", 100) + "\n"
  let expected_tail = repeated("a", 80) + "\n" + repeated("a", 80) + "\n" + repeated("a", 31) + "뉐" + repeated("a", 48) + "\n" + repeated("a", 52) + "\n"
  let r = uu.invoke(s, "fold", ["--characters"], stdin: bytes.from_text(input), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r)
  let segments = r.stdout.utf8()?.split("\n")
  let actual_tail = segments[segments.len() - 5..].join("\n")
  assert actual_tail == expected_tail
}

# origin: uutils test_fold::test_width_zero
test test_uu_fold_width_zero { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fold", ["-w", "0"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "fold: invalid number of columns: '0': Numerical result out of range\n")
}

# origin: uutils test_fold::test_width_overflow
test test_uu_fold_width_overflow { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fold", ["-w", "999999999999999999999"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "fold: invalid number of columns: '999999999999999999999': Numerical result out of range\n")
}

# origin: uutils test_fold::test_width_invalid
test test_uu_fold_width_invalid { |ctx|
  let s = uu.scene(ctx)?
  for width in ["xyz", "12x", "12.5", "1 2"] {
    let r = uu.invoke(s, "fold", ["-w", width])?
    uu.fails_with_code(r, 1)
    uu.stderr_is(r, f"fold: invalid number of columns: '{width}'\n")
  }
}

# origin: uutils test_fold::test_word_boundary_split_should_preserve_empty_lines
test test_uu_fold_word_boundary_split_should_preserve_empty_lines { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fold", ["-s"], stdin: bytes.from_text("\n"))?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("\n"))
  let second = uu.invoke(s, "fold", ["-w1", "-s"], stdin: bytes.from_text("0\n1\n\n2\n\n\n"))?
  uu.succeeds(second)
  uu.stdout_is_bytes(second, bytes.from_text("0\n1\n\n2\n\n\n"))
}

# origin: uutils test_fold::test_zero_width_bytes_in_column_mode
test test_uu_fold_zero_width_bytes_in_column_mode { |ctx|
  let s = uu.scene(ctx)?
  let input = repeated("\x00", 16384)
  let expected = bytes.from_text(input)
  let r = uu.invoke(s, "fold", [], stdin: bytes.from_text(input))?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected)
}

# origin: uutils test_fold::test_zero_width_bytes_in_character_mode
test test_uu_fold_zero_width_bytes_in_character_mode { |ctx|
  let s = uu.scene(ctx)?
  let input = repeated("\x00", 16384)
  let expected = character_folds("\x00", 16384)
  let r = uu.invoke(s, "fold", ["--characters"], stdin: bytes.from_text(input))?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected)
}

# origin: uutils test_fold::test_zero_width_bytes_from_file
test test_uu_fold_zero_width_bytes_from_file { |ctx|
  let s = uu.scene(ctx)?
  let input = repeated("\x00", 16384)
  uu.write(s, "zeros.bin", input)?
  let column = uu.invoke(s, "fold", ["zeros.bin"])?
  uu.succeeds(column)
  uu.stdout_is_bytes(column, bytes.from_text(input))
  let characters = uu.invoke(s, "fold", ["--characters", "zeros.bin"])?
  uu.succeeds(characters)
  uu.stdout_is_bytes(characters, character_folds("\x00", 16384))
}

# origin: uutils test_fold::test_zero_width_spaces_in_column_mode
test test_uu_fold_zero_width_spaces_in_column_mode { |ctx|
  let s = uu.scene(ctx)?
  let input = repeated("\u{200B}", 16384)
  let expected = bytes.from_text(input)
  let r = uu.invoke(s, "fold", [], stdin: bytes.from_text(input), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected)
}

# origin: uutils test_fold::test_zero_width_spaces_in_character_mode
test test_uu_fold_zero_width_spaces_in_character_mode { |ctx|
  let s = uu.scene(ctx)?
  let input = repeated("\u{200B}", 16384)
  let expected = character_folds("\u{200B}", 16384)
  let r = uu.invoke(s, "fold", ["--characters"], stdin: bytes.from_text(input), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, expected)
}

# origin: uutils test_fold::test_zero_width_spaces_from_file
test test_uu_fold_zero_width_spaces_from_file { |ctx|
  let s = uu.scene(ctx)?
  let input = repeated("\u{200B}", 16384)
  uu.write(s, "zero-width.txt", input)?
  let column = uu.invoke(s, "fold", ["zero-width.txt"], vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(column)
  uu.stdout_is_bytes(column, bytes.from_text(input))
  let characters = uu.invoke(s, "fold", ["--characters", "zero-width.txt"], vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(characters)
  uu.stdout_is_bytes(characters, character_folds("\u{200B}", 16384))
}

# origin: uutils test_fold::test_zero_width_data_line_counts
test test_uu_fold_zero_width_data_line_counts { |ctx|
  let s = uu.scene(ctx)?
  for value in ["\x00", "\u{200B}"] {
    let input = bytes.from_text(repeated(value, 16384))
    let column = uu.invoke(s, "fold", [], stdin: input, vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(column)
    assert column.stdout.utf8()?.split("\n").len() - 1 == 0
    let characters = uu.invoke(s, "fold", ["--characters"], stdin: input, vars: {LC_ALL: "C.UTF-8"})?
    uu.succeeds(characters)
    assert characters.stdout.utf8()?.split("\n").len() - 1 == 16384 / 80
  }
}
