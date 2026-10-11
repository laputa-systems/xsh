##! Transcribed from the MIT-licensed uutils fold integration tests.
use support.uu as uu

# Unicode cases need a UTF-8 locale so fold measures decoded characters and columns.

# origin: uutils test_fold::test_40_column_hard_cutoff
test test_uu_fold_40_column_hard_cutoff { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fold", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "fold", ["-w", "40", "lorem_ipsum.txt"])?
  uu.succeeds(r1)
  uu.fixture(s, "fold", "lorem_ipsum_40_column_hard.expected", "lorem_ipsum_40_column_hard.expected")?
  uu.stdout_is_bytes(r1, uu.read(s, "lorem_ipsum_40_column_hard.expected")?)
}

# origin: uutils test_fold::test_40_column_word_boundary
test test_uu_fold_40_column_word_boundary { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fold", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "fold", ["-s", "-w", "40", "lorem_ipsum.txt"])?
  uu.succeeds(r1)
  uu.fixture(s, "fold", "lorem_ipsum_40_column_word.expected", "lorem_ipsum_40_column_word.expected")?
  uu.stdout_is_bytes(r1, uu.read(s, "lorem_ipsum_40_column_word.expected")?)
}

# origin: uutils test_fold::test_all_tab_advances_at_non_utf8_character
test test_uu_fold_all_tab_advances_at_non_utf8_character { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fold", "non_utf8_tab_stops.input", "non_utf8_tab_stops.input")?
  let r1 = uu.invoke(s, "fold", ["-w16", "non_utf8_tab_stops.input"])?
  uu.succeeds(r1)
  uu.fixture(s, "fold", "non_utf8_tab_stops_w16.expected", "non_utf8_tab_stops_w16.expected")?
  uu.stdout_is_bytes(r1, uu.read(s, "non_utf8_tab_stops_w16.expected")?)
}

# origin: uutils test_fold::test_all_tabs_should_advance_to_next_tab_stops
test test_uu_fold_all_tabs_should_advance_to_next_tab_stops { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fold", "tab_stops.input", "tab_stops.input")?
  let r1 = uu.invoke(s, "fold", ["-w16", "tab_stops.input"])?
  uu.succeeds(r1)
  uu.fixture(s, "fold", "tab_stops_w16.expected", "tab_stops_w16.expected")?
  uu.stdout_is_bytes(r1, uu.read(s, "tab_stops_w16.expected")?)
}

# origin: uutils test_fold::test_backspace_is_not_word_boundary
test test_uu_fold_backspace_is_not_word_boundary { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w10", "-s"], stdin: b"foobar\x086789abcdef")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "foobar\x086789a\nbcdef")
}

# origin: uutils test_fold::test_backspace_should_be_preserved
test test_uu_fold_backspace_should_be_preserved { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", [], stdin: b"\x08")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\x08")
}

# origin: uutils test_fold::test_backspace_should_decrease_column_count
test test_uu_fold_backspace_should_decrease_column_count { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2"], stdin: b"1\x08345")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1\x0834\n5")
}

# origin: uutils test_fold::test_backspace_should_not_decrease_column_count_past_zero
test test_uu_fold_backspace_should_not_decrease_column_count_past_zero { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2"], stdin: b"1\x08\x083456")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1\x08\x0834\n56")
}

# origin: uutils test_fold::test_backspaced_char_should_be_preserved
test test_uu_fold_backspaced_char_should_be_preserved { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", [], stdin: b"x\x08")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "x\x08")
}

# origin: uutils test_fold::test_byte_break_at_non_utf8_character
test test_uu_fold_byte_break_at_non_utf8_character { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fold", "non_utf8.input", "non_utf8.input")?
  let r1 = uu.invoke(s, "fold", ["-b", "-s", "-w", "40", "non_utf8.input"])?
  uu.succeeds(r1)
  uu.fixture(s, "fold", "non_utf8.expected", "non_utf8.expected")?
  uu.stdout_is_bytes(r1, uu.read(s, "non_utf8.expected")?)
}

# origin: uutils test_fold::test_bytewise_backspace_is_not_word_boundary
test test_uu_fold_bytewise_backspace_is_not_word_boundary { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w10", "-s", "-b"], stdin: b"foobar\x0889abcdef")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "foobar\x0889a\nbcdef")
}

# origin: uutils test_fold::test_bytewise_backspace_should_be_preserved
test test_uu_fold_bytewise_backspace_should_be_preserved { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-b"], stdin: b"\x08")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\x08")
}

# origin: uutils test_fold::test_bytewise_backspace_should_not_decrease_column_count
test test_uu_fold_bytewise_backspace_should_not_decrease_column_count { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2", "-b"], stdin: b"1\x08345")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1\x08\n34\n5")
}

# origin: uutils test_fold::test_bytewise_backspaced_char_should_be_preserved
test test_uu_fold_bytewise_backspaced_char_should_be_preserved { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-b"], stdin: b"x\x08")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "x\x08")
}

# origin: uutils test_fold::test_bytewise_carriage_return_is_not_word_boundary
test test_uu_fold_bytewise_carriage_return_is_not_word_boundary { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w6", "-s", "-b"], stdin: b"fizz\x0dbuzz\x0dfizzbuzz")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "fizz\rb\nuzz\rfi\nzzbuzz")
}

# origin: uutils test_fold::test_bytewise_carriage_return_overwritten_char_should_be_preserved
test test_uu_fold_bytewise_carriage_return_overwritten_char_should_be_preserved { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-b"], stdin: b"x\x0dy")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "x\ry")
}

# origin: uutils test_fold::test_bytewise_carriage_return_should_be_preserved
test test_uu_fold_bytewise_carriage_return_should_be_preserved { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-b"], stdin: b"\x0d")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\r")
}

# origin: uutils test_fold::test_bytewise_carriage_return_should_not_reset_column_count
test test_uu_fold_bytewise_carriage_return_should_not_reset_column_count { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w6", "-b"], stdin: b"12345\x0d123456789abcdef")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "12345\r\n123456\n789abc\ndef")
}

# origin: uutils test_fold::test_bytewise_fold_at_word_boundary_only_whitespace
test test_uu_fold_bytewise_fold_at_word_boundary_only_whitespace { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2", "-s", "-b"], stdin: b"    ")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "  \n  ")
}

# origin: uutils test_fold::test_bytewise_fold_at_word_boundary_only_whitespace_preserve_final_newline
test test_uu_fold_bytewise_fold_at_word_boundary_only_whitespace_preserve_final_newline { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2", "-s", "-b"], stdin: b"    \x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "  \n  \n")
}

# origin: uutils test_fold::test_bytewise_fold_before_tab_with_narrow_width
test test_uu_fold_bytewise_fold_before_tab_with_narrow_width { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w7", "-b"], stdin: b"a\x091")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\t1")
}

# origin: uutils test_fold::test_bytewise_fold_line_longer_than_width_still_folds
test test_uu_fold_bytewise_fold_line_longer_than_width_still_folds { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w7", "-s", "-b"], stdin: b"aaa bbbb\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "aaa \nbbbb\n")
}

# origin: uutils test_fold::test_bytewise_fold_line_of_exactly_width_is_not_folded
test test_uu_fold_bytewise_fold_line_of_exactly_width_is_not_folded { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w7", "-s", "-b"], stdin: b"aaa bbb\x0accc ddd\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "aaa bbb\nccc ddd\n")
}

# origin: uutils test_fold::test_bytewise_fold_remainder_of_exactly_width_is_not_folded
test test_uu_fold_bytewise_fold_remainder_of_exactly_width_is_not_folded { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w7", "-s", "-b"], stdin: b"aaa bbb ccc\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "aaa \nbbb ccc\n")
}

# origin: uutils test_fold::test_bytewise_should_not_add_newline_when_line_equal_to_fold
test test_uu_fold_bytewise_should_not_add_newline_when_line_equal_to_fold { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w1", "-b"], stdin: b" ")?
  uu.succeeds(r1)
  uu.stdout_is(r1, " ")
}

# origin: uutils test_fold::test_bytewise_should_not_add_newline_when_line_less_than_fold
test test_uu_fold_bytewise_should_not_add_newline_when_line_less_than_fold { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-b"], stdin: b"1234")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1234")
}

# origin: uutils test_fold::test_bytewise_should_not_add_newline_when_line_longer_than_fold
test test_uu_fold_bytewise_should_not_add_newline_when_line_longer_than_fold { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2", "-b"], stdin: b"1234")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "12\n34")
}

# origin: uutils test_fold::test_bytewise_should_preserve_empty_line_and_final_newline
test test_uu_fold_bytewise_should_preserve_empty_line_and_final_newline { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2", "-b"], stdin: b"12\x0a\x0a34\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "12\n\n34\n")
}

# origin: uutils test_fold::test_bytewise_should_preserve_empty_line_without_final_newline
test test_uu_fold_bytewise_should_preserve_empty_line_without_final_newline { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2", "-b"], stdin: b"123\x0a\x0a45")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "12\n3\n\n45")
}

# origin: uutils test_fold::test_bytewise_should_preserve_empty_lines
test test_uu_fold_bytewise_should_preserve_empty_lines { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-b"], stdin: b"\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\n")
  let r2 = uu.invoke(s, "fold", ["-w1", "-b"], stdin: b"0\x0a1\x0a\x0a2\x0a\x0a\x0a")?
  uu.succeeds(r2)
  uu.stdout_is(r2, "0\n1\n\n2\n\n\n")
}

# origin: uutils test_fold::test_bytewise_should_preserve_final_newline_when_line_equal_to_fold
test test_uu_fold_bytewise_should_preserve_final_newline_when_line_equal_to_fold { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2", "-b"], stdin: b"1\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1\n")
}

# origin: uutils test_fold::test_bytewise_should_preserve_final_newline_when_line_less_than_fold
test test_uu_fold_bytewise_should_preserve_final_newline_when_line_less_than_fold { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-b"], stdin: b"1234\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1234\n")
}

# origin: uutils test_fold::test_bytewise_should_preserve_final_newline_when_line_longer_than_fold
test test_uu_fold_bytewise_should_preserve_final_newline_when_line_longer_than_fold { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2", "-b"], stdin: b"1234\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "12\n34\n")
}

# origin: uutils test_fold::test_bytewise_single_tab_should_not_add_extra_newline
test test_uu_fold_bytewise_single_tab_should_not_add_extra_newline { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w1", "-b"], stdin: b"\x09")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t")
}

# origin: uutils test_fold::test_bytewise_word_boundary_split_should_preserve_empty_lines
test test_uu_fold_bytewise_word_boundary_split_should_preserve_empty_lines { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-s", "-b"], stdin: b"\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\n")
  let r2 = uu.invoke(s, "fold", ["-w1", "-s", "-b"], stdin: b"0\x0a1\x0a\x0a2\x0a\x0a\x0a")?
  uu.succeeds(r2)
  uu.stdout_is(r2, "0\n1\n\n2\n\n\n")
}

# origin: uutils test_fold::test_carriage_return_is_not_word_boundary
test test_uu_fold_carriage_return_is_not_word_boundary { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w6", "-s"], stdin: b"fizz\x0dbuzz\x0dfizzbuzz")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "fizz\rbuzz\rfizzbu\nzz")
}

# origin: uutils test_fold::test_carriage_return_overwritten_char_should_be_preserved
test test_uu_fold_carriage_return_overwritten_char_should_be_preserved { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", [], stdin: b"x\x0dy")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "x\ry")
}

# origin: uutils test_fold::test_carriage_return_should_be_preserved
test test_uu_fold_carriage_return_should_be_preserved { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", [], stdin: b"\x0d")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\r")
}

# origin: uutils test_fold::test_carriage_return_should_reset_column_count
test test_uu_fold_carriage_return_should_reset_column_count { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w6"], stdin: b"12345\x0d123456789abcdef")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "12345\r123456\n789abc\ndef")
}

# origin: uutils test_fold::test_dash_operand_does_not_swallow_the_next_argument
test test_uu_fold_dash_operand_does_not_swallow_the_next_argument { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-", "-5"], stdin: b"hello\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "hello\n")
}

# origin: uutils test_fold::test_dash_operand_reads_stdin
test test_uu_fold_dash_operand_reads_stdin { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-"], stdin: b"hello\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "hello\n")
  let r2 = uu.invoke(s, "fold", ["-w", "3", "-"], stdin: b"hello\x0a")?
  uu.succeeds(r2)
  uu.stdout_is(r2, "hel\nlo\n")
  let r3 = uu.invoke(s, "fold", ["-w", "-"], stdin: b"hello\x0a")?
  uu.fails(r3)
  uu.stderr_contains(r3, "invalid number of columns: '-'")
}

# origin: uutils test_fold::test_default_80_column_wrap
test test_uu_fold_default_80_column_wrap { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fold", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "fold", ["lorem_ipsum.txt"])?
  uu.succeeds(r1)
  uu.fixture(s, "fold", "lorem_ipsum_80_column.expected", "lorem_ipsum_80_column.expected")?
  uu.stdout_is_bytes(r1, uu.read(s, "lorem_ipsum_80_column.expected")?)
}

# origin: uutils test_fold::test_default_wrap_with_newlines
test test_uu_fold_default_wrap_with_newlines { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fold", "lorem_ipsum_new_line.txt", "lorem_ipsum_new_line.txt")?
  let r1 = uu.invoke(s, "fold", ["lorem_ipsum_new_line.txt"])?
  uu.succeeds(r1)
  uu.fixture(s, "fold", "lorem_ipsum_new_line_80_column.expected", "lorem_ipsum_new_line_80_column.expected")?
  uu.stdout_is_bytes(r1, uu.read(s, "lorem_ipsum_new_line_80_column.expected")?)
}

# origin: uutils test_fold::test_fold_after_tab
test test_uu_fold_fold_after_tab { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w10"], stdin: b"a\x09bbb\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\tbb\nb\n")
}

# origin: uutils test_fold::test_fold_after_tab_as_word_boundary
test test_uu_fold_fold_after_tab_as_word_boundary { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w10", "-s"], stdin: b"a\x09bbb\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\t\nbbb\n")
}

# origin: uutils test_fold::test_fold_at_leading_word_boundary
test test_uu_fold_fold_at_leading_word_boundary { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w3", "-s"], stdin: b" aaa")?
  uu.succeeds(r1)
  uu.stdout_is(r1, " \naaa")
}

# origin: uutils test_fold::test_fold_at_tab
test test_uu_fold_fold_at_tab { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w8"], stdin: b"a\x09bbb\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\t\nbbb\n")
}

# origin: uutils test_fold::test_fold_at_tab_as_word_boundary
test test_uu_fold_fold_at_tab_as_word_boundary { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w8", "-s"], stdin: b"a\x09bbb\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\t\nbbb\n")
}

# origin: uutils test_fold::test_fold_at_word_boundary
test test_uu_fold_fold_at_word_boundary { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w4", "-s"], stdin: b"one two")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "one \ntwo")
}

# origin: uutils test_fold::test_fold_at_word_boundary_only_whitespace
test test_uu_fold_fold_at_word_boundary_only_whitespace { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2", "-s"], stdin: b"    ")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "  \n  ")
}

# origin: uutils test_fold::test_fold_at_word_boundary_only_whitespace_preserve_final_newline
test test_uu_fold_fold_at_word_boundary_only_whitespace_preserve_final_newline { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2", "-s"], stdin: b"    \x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "  \n  \n")
}

# origin: uutils test_fold::test_fold_at_word_boundary_preserve_final_newline
test test_uu_fold_fold_at_word_boundary_preserve_final_newline { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w4", "-s"], stdin: b"one two\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "one \ntwo\n")
}

# origin: uutils test_fold::test_fold_before_tab_with_narrow_width
test test_uu_fold_fold_before_tab_with_narrow_width { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w7"], stdin: b"a\x091")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a\n\t\n1")
}

# origin: uutils test_fold::test_fold_characters_tab_advances_to_next_tab_stop
test test_uu_fold_fold_characters_tab_advances_to_next_tab_stop { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-c", "-w", "4"], stdin: b"ab\x09cd\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "ab\n\t\ncd\n")
}

# origin: uutils test_fold::test_fold_characters_tab_with_non_ascii
test test_uu_fold_fold_characters_tab_with_non_ascii { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-c", "-w", "2"], stdin: b"\xc3\xa9\x09b\x0a", vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r1)
  uu.stdout_is(r1, "é\n\t\nb\n")
}

# origin: uutils test_fold::test_initial_tab_counts_as_8_columns
test test_uu_fold_initial_tab_counts_as_8_columns { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w8"], stdin: b"\x091")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\t\n1")
}

# origin: uutils test_fold::test_invalid_arg
test test_uu_fold_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["--definitely-invalid"])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_fold::test_obsolete_syntax
test test_uu_fold_obsolete_syntax { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fold", "space_separated_words.txt", "space_separated_words.txt")?
  let r1 = uu.invoke(s, "fold", ["-5", "-s", "space_separated_words.txt"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "test1\n \ntest2\n \ntest3\n \ntest4\n \ntest5\n \ntest6\n ")
}

# origin: uutils test_fold::test_obsolete_width_after_double_dash_is_a_file
test test_uu_fold_obsolete_width_after_double_dash_is_a_file { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["--", "-3"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_is(r1, "fold: -3: No such file or directory\n")
}

# origin: uutils test_fold::test_should_not_add_newline_when_line_equal_to_fold
test test_uu_fold_should_not_add_newline_when_line_equal_to_fold { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w1"], stdin: b" ")?
  uu.succeeds(r1)
  uu.stdout_is(r1, " ")
}

# origin: uutils test_fold::test_should_not_add_newline_when_line_less_than_fold
test test_uu_fold_should_not_add_newline_when_line_less_than_fold { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", [], stdin: b"1234")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1234")
}

# origin: uutils test_fold::test_should_not_add_newline_when_line_longer_than_fold
test test_uu_fold_should_not_add_newline_when_line_longer_than_fold { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2"], stdin: b"1234")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "12\n34")
}

# origin: uutils test_fold::test_should_preserve_empty_line_and_final_newline
test test_uu_fold_should_preserve_empty_line_and_final_newline { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2"], stdin: b"12\x0a\x0a34\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "12\n\n34\n")
}

# origin: uutils test_fold::test_should_preserve_empty_line_without_final_newline
test test_uu_fold_should_preserve_empty_line_without_final_newline { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2"], stdin: b"12\x0a\x0a34")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "12\n\n34")
}

# origin: uutils test_fold::test_should_preserve_empty_lines
test test_uu_fold_should_preserve_empty_lines { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", [], stdin: b"\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "\n")
  let r2 = uu.invoke(s, "fold", ["-w1"], stdin: b"0\x0a1\x0a\x0a2\x0a\x0a\x0a")?
  uu.succeeds(r2)
  uu.stdout_is(r2, "0\n1\n\n2\n\n\n")
}

# origin: uutils test_fold::test_should_preserve_final_newline_when_line_equal_to_fold
test test_uu_fold_should_preserve_final_newline_when_line_equal_to_fold { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2"], stdin: b"1\x0a")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1\n")
}

# origin: uutils test_fold::test_bytewise_fold_at_read_buffer_boundary
test test_uu_fold_bytewise_fold_at_read_buffer_boundary { |ctx|
  let s = uu.scene(ctx)?
  # The upstream Rust reader capacity is 8192 bytes on this target.
  let half = ["a" for _ in range(0, 8192)].join("")
  let r1 = uu.invoke(s, "fold", ["-b", "-w8192"], stdin: bytes.from_text(f"{half}{half}"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, f"{half}\n{half}")
}

# origin: uutils test_fold::test_bytewise_read_from_pseudo_device
test test_uu_fold_bytewise_read_from_pseudo_device { |ctx|
  let s = uu.scene(ctx)?
  let out = uu.at(s, "output")
  let err = uu.at(s, "error")
  let plan = uu.command(s, "fold", ["-b", "/dev/zero"], stdout: out, stderr: err)?
  let child = spawn plan?
  defer child.cancel(signal: "KILL", kill_after: 0ms)?
  time.sleep(100ms)?
  assert process.wait_timeout([child], 0ms)? == null
  process.kill(child.pid, signal: "KILL")?
  let done = process.wait_timeout([child], 2s)?
  assert done != null
  assert b"\x00\x0a" in out.read_bytes()?
  assert err.read_bytes()? == b""
}

# origin: uutils test_fold::test_character_mode_special_chars
test test_uu_fold_character_mode_special_chars { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-c", "-w", "5"], stdin: bytes.from_text("abcde\x08fg\n"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r1)
  uu.stdout_is(r1, "abcde\x08f\ng\n")
  let r2 = uu.invoke(s, "fold", ["-c", "-w", "5"], stdin: bytes.from_text("abcd\refgh\n"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r2)
  uu.stdout_is(r2, "abcd\refgh\n")
  let r3 = uu.invoke(s, "fold", ["-c", "-w", "4"], stdin: bytes.from_text("\tabc\n"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r3)
  uu.stdout_is(r3, "\t\nabc\n")
  let r4 = uu.invoke(s, "fold", ["-c", "-w", "10"], stdin: bytes.from_text("a\tb\tc\n"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r4)
  uu.stdout_is(r4, "a\tb\n\tc\n")
  let r5 = uu.invoke(s, "fold", ["-c", "-w", "3"], stdin: bytes.from_text("abcdef\n"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r5)
  uu.stdout_is(r5, "abc\ndef\n")
  let r6 = uu.invoke(s, "fold", ["-c", "-w", "5"], stdin: bytes.from_text("abc\n\ndef\n"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r6)
  uu.stdout_is(r6, "abc\n\ndef\n")
  let r7 = uu.invoke(s, "fold", ["-c", "-s", "-w", "5"], stdin: bytes.from_text("ab cd ef\n"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r7)
  uu.stdout_is(r7, "ab \ncd ef\n")
  let r8 = uu.invoke(s, "fold", ["-c", "-s", "-w", "10"], stdin: bytes.from_text("abcd\tefgh\n"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r8)
  uu.stdout_is(r8, "abcd\t\nefgh\n")
  let r9 = uu.invoke(s, "fold", ["-c", "-w", "3"], stdin: bytes.from_text("：：：：\n"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r9)
  uu.stdout_is(r9, "：：：\n：\n")
}

# origin: uutils test_fold::test_combining_characters_nfc
test test_uu_fold_combining_characters_nfc { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2"], stdin: bytes.from_text("ééé"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r1)
  uu.stdout_is(r1, "éé\né")
}

# origin: uutils test_fold::test_combining_characters_nfd
test test_uu_fold_combining_characters_nfd { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2"], stdin: bytes.from_text("ééé"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r1)
  uu.stdout_is(r1, "éé\né")
}

# origin: uutils test_fold::test_continue_after_missing_file
test test_uu_fold_continue_after_missing_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "first.txt", "hello\n")?
  uu.write(s, "third.txt", "world\n")?
  let r1 = uu.invoke(s, "fold", ["first.txt", "absent.txt", "third.txt"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_is(r1, "hello\nworld\n")
  uu.stderr_is(r1, "fold: absent.txt: No such file or directory\n")
}

# origin: uutils test_fold::test_fold_preserves_incomplete_utf8_at_eof
test test_uu_fold_fold_preserves_incomplete_utf8_at_eof { |ctx|
  let s = uu.scene(ctx)?
  let input = b"\xC3"
  let r1 = uu.invoke(s, "fold", [], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, input)
}

# origin: uutils test_fold::test_fold_preserves_invalid_utf8_sequences
test test_uu_fold_fold_preserves_invalid_utf8_sequences { |ctx|
  let s = uu.scene(ctx)?
  let input = b"\xC3|\xED\xBA\xAD|\x00|\x89|\xED\xA6\xBF\xED\xBF\xBF\n"
  let r1 = uu.invoke(s, "fold", [], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, input)
}

# origin: uutils test_fold::test_fold_reports_no_space_left_on_dev_full
test test_uu_fold_fold_reports_no_space_left_on_dev_full { |ctx|
  let s = uu.scene(ctx)?
  for byte in [b"\n", b"\x00", b"\xc3"] {
    let input = bytes.concat([byte for _ in range(0, 1024)])
    let r1 = uu.invoke(s, "fold", [], stdin: input, stdout: p"/dev/full")?
    uu.fails(r1)
    uu.stderr_contains(r1, "No space left")
  }
}

# origin: uutils test_fold::test_fullwidth_characters
test test_uu_fold_fullwidth_characters { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w2"], stdin: bytes.from_text("ｅｅ"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r1)
  uu.stdout_is(r1, "ｅ\nｅ")
}

# origin: uutils test_fold::test_last_width_wins
test test_uu_fold_last_width_wins { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w3", "-5"], stdin: bytes.from_text("aaaaaaaaaa\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "aaaaa\naaaaa\n")
  let r2 = uu.invoke(s, "fold", ["-3", "-5"], stdin: bytes.from_text("aaaaaaaaaa\n"))?
  uu.succeeds(r2)
  uu.stdout_is(r2, "aaaaa\naaaaa\n")
  let r3 = uu.invoke(s, "fold", ["-w", "3", "-w", "5"], stdin: bytes.from_text("aaaaaaaaaa\n"))?
  uu.succeeds(r3)
  uu.stdout_is(r3, "aaaaa\naaaaa\n")
  let r4 = uu.invoke(s, "fold", ["-w3", "-w5"], stdin: bytes.from_text("aaaaaaaaaa\n"))?
  uu.succeeds(r4)
  uu.stdout_is(r4, "aaaaa\naaaaa\n")
  let r5 = uu.invoke(s, "fold", ["-5", "-w", "5"], stdin: bytes.from_text("aaaaaaaaaa\n"))?
  uu.succeeds(r5)
  uu.stdout_is(r5, "aaaaa\naaaaa\n")
  let r6 = uu.invoke(s, "fold", ["-5", "-w", "3"], stdin: bytes.from_text("aaaaaa\n"))?
  uu.succeeds(r6)
  uu.stdout_is(r6, "aaa\naaa\n")
}

# origin: uutils test_fold::test_multiple_wide_characters_in_character_mode
test test_uu_fold_multiple_wide_characters_in_character_mode { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["--characters", "-w", "10"], stdin: bytes.from_text("：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：\n"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r1)
  uu.stdout_is(r1, "：：：：：：：：：：\n：：：：：：：：：：\n：：：：：：：：：：\n：：：：：：：：：：\n：：：：：：：：：：\n")
}

# origin: uutils test_fold::test_multiple_wide_characters_in_column_mode
test test_uu_fold_multiple_wide_characters_in_column_mode { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "fold", ["-w", "10"], stdin: bytes.from_text("：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：：\n"), vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(r1)
  uu.stdout_is(r1, "：：：：：\n：：：：：\n：：：：：\n：：：：：\n：：：：：\n：：：：：\n：：：：：\n：：：：：\n：：：：：\n：：：：：\n")
}

# origin: uutils test_fold::test_negative_width_as_separate_value
test test_uu_fold_negative_width_as_separate_value { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "fold", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r1 = uu.invoke(s, "fold", ["-w", "-1", "lorem_ipsum.txt"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_is(r1, "fold: invalid number of columns: '-1'\n")
  let r2 = uu.invoke(s, "fold", ["--width", "-1", "lorem_ipsum.txt"])?
  uu.fails_with_code(r2, 1)
  uu.stderr_is(r2, "fold: invalid number of columns: '-1'\n")
  let r3 = uu.invoke(s, "fold", ["-bw", "-1", "lorem_ipsum.txt"])?
  uu.fails_with_code(r3, 1)
  uu.stderr_is(r3, "fold: invalid number of columns: '-1'\n")
  let r4 = uu.invoke(s, "fold", ["-sw", "-1", "lorem_ipsum.txt"])?
  uu.fails_with_code(r4, 1)
  uu.stderr_is(r4, "fold: invalid number of columns: '-1'\n")
}
