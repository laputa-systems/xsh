##! Transcribed from the MIT-licensed uutils ptx integration tests.
use support.uu as uu

# origin: uutils test_ptx::gnu_ext_disabled_break_file
test test_uu_ptx_gnu_ext_disabled_break_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "break_file", "break_file")?
  uu.fixture(s, "ptx", "input", "input")?
  let r1 = uu.invoke(s, "ptx", ["-G", "-b", "break_file", "input"], stdin: b"")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "gnu_ext_disabled_break_file.expected", "gnu_ext_disabled_break_file.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "gnu_ext_disabled_break_file.expected")?)
}

# origin: uutils test_ptx::gnu_ext_disabled_chunk_no_over_reading
test test_uu_ptx_gnu_ext_disabled_chunk_no_over_reading { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-G", "-w", "30"], stdin: b"Hello World Rust is a fun language")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "gnu_ext_disabled_chunk_no_over_reading.expected", "gnu_ext_disabled_chunk_no_over_reading.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "gnu_ext_disabled_chunk_no_over_reading.expected")?)
}

# origin: uutils test_ptx::gnu_ext_disabled_empty_word_regexp_ignores_break_file
test test_uu_ptx_gnu_ext_disabled_empty_word_regexp_ignores_break_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "break_file", "break_file")?
  uu.fixture(s, "ptx", "input", "input")?
  let r1 = uu.invoke(s, "ptx", ["-G", "-b", "break_file", "-R", "-W", "", "input"], stdin: b"")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "gnu_ext_disabled_empty_word_regexp_ignores_break_file.gnu.expected", "gnu_ext_disabled_empty_word_regexp_ignores_break_file.gnu.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "gnu_ext_disabled_empty_word_regexp_ignores_break_file.gnu.expected")?)
}

# origin: uutils test_ptx::gnu_ext_disabled_ignore_and_only_file
test test_uu_ptx_gnu_ext_disabled_ignore_and_only_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "only", "only")?
  uu.fixture(s, "ptx", "ignore", "ignore")?
  uu.fixture(s, "ptx", "input", "input")?
  let r1 = uu.invoke(s, "ptx", ["-G", "-o", "only", "-i", "ignore", "input"], stdin: b"")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "gnu_ext_disabled_ignore_and_only_file.expected", "gnu_ext_disabled_ignore_and_only_file.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "gnu_ext_disabled_ignore_and_only_file.expected")?)
}

# origin: uutils test_ptx::gnu_ext_disabled_output_width_50
test test_uu_ptx_gnu_ext_disabled_output_width_50 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "input", "input")?
  let r1 = uu.invoke(s, "ptx", ["-G", "-w", "50", "input"], stdin: b"")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "gnu_ext_disabled_output_width_50.expected", "gnu_ext_disabled_output_width_50.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "gnu_ext_disabled_output_width_50.expected")?)
}

# origin: uutils test_ptx::gnu_ext_disabled_output_width_70
test test_uu_ptx_gnu_ext_disabled_output_width_70 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "input", "input")?
  let r1 = uu.invoke(s, "ptx", ["-G", "-w", "70", "input"], stdin: b"")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "gnu_ext_disabled_output_width_70.expected", "gnu_ext_disabled_output_width_70.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "gnu_ext_disabled_output_width_70.expected")?)
}

# origin: uutils test_ptx::gnu_ext_disabled_reference_calculation
test test_uu_ptx_gnu_ext_disabled_reference_calculation { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-G", "-A"], stdin: b"Hello World Rust is good language")?
  uu.succeeds(r1)
  uu.stdout_only(r1, ".xx \"language\" \"\" \"Hello World Rust is good\" \"\" \":1\"\n.xx \"\" \"Hello World\" \"Rust is good language\" \"\" \":1\"\n.xx \"\" \"Hello\" \"World Rust is good language\" \"\" \":1\"\n.xx \"\" \"Hello World Rust is\" \"good language\" \"\" \":1\"\n.xx \"\" \"Hello World Rust\" \"is good language\" \"\" \":1\"\n.xx \"\" \"Hello World Rust is good\" \"language\" \"\" \":1\"\n")
}

# origin: uutils test_ptx::gnu_ext_disabled_rightward_auto_ref
test test_uu_ptx_gnu_ext_disabled_rightward_auto_ref { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "input", "input")?
  let r1 = uu.invoke(s, "ptx", ["-G", "-A", "-R", "input"], stdin: b"")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "gnu_ext_disabled_rightward_auto_ref.expected", "gnu_ext_disabled_rightward_auto_ref.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "gnu_ext_disabled_rightward_auto_ref.expected")?)
}

# origin: uutils test_ptx::gnu_ext_disabled_rightward_input_ref
test test_uu_ptx_gnu_ext_disabled_rightward_input_ref { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "input", "input")?
  let r1 = uu.invoke(s, "ptx", ["-G", "-r", "-R", "input"], stdin: b"")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "gnu_ext_disabled_rightward_input_ref.expected", "gnu_ext_disabled_rightward_input_ref.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "gnu_ext_disabled_rightward_input_ref.expected")?)
}

# origin: uutils test_ptx::gnu_ext_disabled_rightward_no_ref
test test_uu_ptx_gnu_ext_disabled_rightward_no_ref { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "input", "input")?
  let r1 = uu.invoke(s, "ptx", ["-G", "-R", "input"], stdin: b"")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "gnu_ext_disabled_rightward_no_ref.expected", "gnu_ext_disabled_rightward_no_ref.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "gnu_ext_disabled_rightward_no_ref.expected")?)
}

# origin: uutils test_ptx::gnu_ext_disabled_rightward_no_ref_empty_word_regexp
test test_uu_ptx_gnu_ext_disabled_rightward_no_ref_empty_word_regexp { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "input", "input")?
  let r1 = uu.invoke(s, "ptx", ["-G", "-R", "-W", "", "input"], stdin: b"")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "gnu_ext_disabled_rightward_no_ref.expected", "gnu_ext_disabled_rightward_no_ref.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "gnu_ext_disabled_rightward_no_ref.expected")?)
}

# origin: uutils test_ptx::gnu_ext_disabled_rightward_no_ref_word_regexp_exc_space
test test_uu_ptx_gnu_ext_disabled_rightward_no_ref_word_regexp_exc_space { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "input", "input")?
  let r1 = uu.invoke(s, "ptx", ["-G", "-R", "-W", "[^\t\n]+", "input"], stdin: b"")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "gnu_ext_disabled_rightward_no_ref_word_regexp_exc_space.expected", "gnu_ext_disabled_rightward_no_ref_word_regexp_exc_space.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "gnu_ext_disabled_rightward_no_ref_word_regexp_exc_space.expected")?)
}

# origin: uutils test_ptx::gnu_ext_disabled_tex_auto_ref
test test_uu_ptx_gnu_ext_disabled_tex_auto_ref { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "input", "input")?
  let r1 = uu.invoke(s, "ptx", ["-G", "-T", "-A", "-R", "input"], stdin: b"")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "gnu_ext_disabled_tex_auto_ref.expected", "gnu_ext_disabled_tex_auto_ref.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "gnu_ext_disabled_tex_auto_ref.expected")?)
}

# origin: uutils test_ptx::gnu_ext_disabled_tex_input_ref
test test_uu_ptx_gnu_ext_disabled_tex_input_ref { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "input", "input")?
  let r1 = uu.invoke(s, "ptx", ["-G", "-T", "-r", "-R", "input"], stdin: b"")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "gnu_ext_disabled_tex_input_ref.expected", "gnu_ext_disabled_tex_input_ref.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "gnu_ext_disabled_tex_input_ref.expected")?)
}

# origin: uutils test_ptx::gnu_ext_disabled_tex_no_ref
test test_uu_ptx_gnu_ext_disabled_tex_no_ref { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "input", "input")?
  let r1 = uu.invoke(s, "ptx", ["-G", "-T", "-R", "input"], stdin: b"")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "gnu_ext_disabled_tex_no_ref.expected", "gnu_ext_disabled_tex_no_ref.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "gnu_ext_disabled_tex_no_ref.expected")?)
}

# origin: uutils test_ptx::test_break_file_regex_escaping
test test_uu_ptx_break_file_regex_escaping { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "input", "input")?
  let r1 = uu.invoke(s, "ptx", ["-G", "-b", "-", "input"], stdin: b"\x5c.+*?()|[]{}^$#&-~")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "break_file_regex_escaping.expected", "break_file_regex_escaping.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "break_file_regex_escaping.expected")?)
}

# origin: uutils test_ptx::test_default_width_72
test test_uu_ptx_default_width_72 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", [], stdin: b"bar\x0a")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "                                       bar\n")
}

# origin: uutils test_ptx::test_duplicate_input_files
test test_uu_ptx_duplicate_input_files { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "ptx", "one_word", "one_word")?
  let r1 = uu.invoke(s, "ptx", ["one_word", "one_word"], stdin: b"")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "                                       rust\n                                       rust\n")
}

# origin: uutils test_ptx::test_format
test test_uu_ptx_format { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-G", "-O"], stdin: b"a")?
  uu.succeeds(r1)
  uu.stdout_only(r1, ".xx \"\" \"\" \"a\" \"\"\n")
  let r2 = uu.invoke(s, "ptx", ["-G", "-T"], stdin: b"a")?
  uu.succeeds(r2)
  uu.stdout_only(r2, "\\xx {}{}{a}{}{}\n")
  let r3 = uu.invoke(s, "ptx", ["-G", "--format=roff"], stdin: b"a")?
  uu.succeeds(r3)
  uu.stdout_only(r3, ".xx \"\" \"\" \"a\" \"\"\n")
  let r4 = uu.invoke(s, "ptx", ["-G", "--format=tex"], stdin: b"a")?
  uu.succeeds(r4)
  uu.stdout_only(r4, "\\xx {}{}{a}{}{}\n")
}

# origin: uutils test_ptx::test_gnu_compat_numeric_token_with_emoji_produces_no_index
test test_uu_ptx_gnu_compat_numeric_token_with_emoji_produces_no_index { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", [], stdin: b"012345678901234567890123456789\xf0\x9f\x9b\xa0\x0a")?
  uu.succeeds(r1)
  uu.no_output(r1)
}

# origin: uutils test_ptx::test_gnu_compatibility_narrow_width
test test_uu_ptx_gnu_compatibility_narrow_width { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-w", "2"], stdin: b"qux")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "      qux\n")
}

# origin: uutils test_ptx::test_gnu_compatibility_truncation_width
test test_uu_ptx_gnu_compatibility_truncation_width { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-w", "10"], stdin: b"foo bar")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "     /   bar\n        foo/\n")
}

# origin: uutils test_ptx::test_gnu_mode_dumb_format
test test_uu_ptx_gnu_mode_dumb_format { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", [], stdin: b"a b")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "                                       a b\n                                   a   b\n")
  let r2 = uu.invoke(s, "ptx", [], stdin: b"2a")?
  uu.succeeds(r2)
  uu.stdout_only(r2, "                                   2   a\n")
}

# origin: uutils test_ptx::test_ignore_case
test test_uu_ptx_ignore_case { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-G", "-f"], stdin: b"a _")?
  uu.succeeds(r1)
  uu.stdout_only(r1, ".xx \"\" \"\" \"a _\" \"\"\n.xx \"\" \"a\" \"_\" \"\"\n")
}

# origin: uutils test_ptx::test_invalid_arg
test test_uu_ptx_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["--definitely-invalid"], stdin: b"")?
  uu.fails_with_code(r1, 1)
  let r2 = uu.invoke(s, "ptx", ["-g", "0"], stdin: b"")?
  uu.fails_with_code(r2, 1)
  let r3 = uu.invoke(s, "ptx", ["-w", "0"], stdin: b"")?
  uu.fails_with_code(r3, 1)
}

# origin: uutils test_ptx::test_invalid_regex_word_trailing_backslash
test test_uu_ptx_invalid_regex_word_trailing_backslash { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-W", "bar\\"], stdin: b"")?
  uu.succeeds(r1)
  uu.no_stderr(r1)
}

# origin: uutils test_ptx::test_invalid_regex_word_unclosed_group
test test_uu_ptx_invalid_regex_word_unclosed_group { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-W", "(wrong"], stdin: b"")?
  uu.succeeds(r1)
  uu.no_stderr(r1)
}

# origin: uutils test_ptx::test_invalid_utf8_input_is_not_an_error
test test_uu_ptx_invalid_utf8_input_is_not_an_error { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", [], stdin: b"ab\xffcd\x0a")?
  uu.succeeds(r1)
  uu.no_stderr(r1)
}

# origin: uutils test_ptx::test_missing_file_error_contains_filename
test test_uu_ptx_missing_file_error_contains_filename { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["zxc"], stdin: b"")?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "ptx: zxc: No such file or directory\n")
}

# origin: uutils test_ptx::test_narrow_width_with_long_reference_no_panic
test test_uu_ptx_narrow_width_with_long_reference_no_panic { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-w", "1", "-A"], stdin: b"content")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "test_narrow_width_with_long_reference_no_panic.gnu.expected", "test_narrow_width_with_long_reference_no_panic.gnu.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "test_narrow_width_with_long_reference_no_panic.gnu.expected")?)
}

# origin: uutils test_ptx::test_nullable_word_regexp_no_empty_matches
test test_uu_ptx_nullable_word_regexp_no_empty_matches { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-W", "[ab]{0,}"], stdin: b"aa bb cc\x0a")?
  uu.succeeds(r1)
  uu.no_output(r1)
}

# origin: uutils test_ptx::test_reference_format_for_stdin
test test_uu_ptx_reference_format_for_stdin { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-G", "-A"], stdin: b"Rust is good language")?
  uu.succeeds(r1)
  uu.stdout_only(r1, ".xx \"\" \"\" \"Rust is good language\" \"\" \":1\"\n.xx \"\" \"Rust is\" \"good language\" \"\" \":1\"\n.xx \"\" \"Rust\" \"is good language\" \"\" \":1\"\n.xx \"\" \"Rust is good\" \"language\" \"\" \":1\"\n")
}

# origin: uutils test_ptx::test_reject_too_many_operands
test test_uu_ptx_reject_too_many_operands { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-G", "-", "-", "-"], stdin: b"")?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_ptx::test_sentence_regex_trailing_backslash
test test_uu_ptx_sentence_regex_trailing_backslash { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-S", "paris\\"], stdin: b"")?
  uu.succeeds(r1)
  uu.no_output(r1)
  let r2 = uu.invoke(s, "ptx", ["-S", "london\\\\\\"], stdin: b"")?
  uu.succeeds(r2)
  uu.no_output(r2)
}

# origin: uutils test_ptx::test_sentence_regexp_basic
test test_uu_ptx_sentence_regexp_basic { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-G", "-S", "\\."], stdin: b"Hello. World.")?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "Hello")
  uu.stdout_contains(r1, "World")
}

# origin: uutils test_ptx::test_sentence_regexp_empty_match_failure
test test_uu_ptx_sentence_regexp_empty_match_failure { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-G", "-S", "^"], stdin: b"")?
  uu.succeeds(r1)
  uu.no_output(r1)
}

# origin: uutils test_ptx::test_sentence_regexp_invalid_syntax_failure
test test_uu_ptx_sentence_regexp_invalid_syntax_failure { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-S", "^["], stdin: b"")?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "ptx: Invalid regular expression (for regexp '^[')\n")
}

# origin: uutils test_ptx::test_sentence_regexp_newlines_are_spaces
test test_uu_ptx_sentence_regexp_newlines_are_spaces { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-G", "-S", "\\."], stdin: b"Start of\x0asentence.")?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "Start of sentence")
}

# origin: uutils test_ptx::test_sentence_regexp_split_behavior
test test_uu_ptx_sentence_regexp_split_behavior { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-G", "-w", "50", "-S", "[.!]"], stdin: b"One sentence. Two sentence!")?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "One sentence")
  uu.stdout_contains(r1, "Two sentence")
}

# origin: uutils test_ptx::test_tex_format_no_truncation_markers
test test_uu_ptx_tex_format_no_truncation_markers { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-G", "-w", "30", "--format=tex"], stdin: b"Hello world Rust is a fun language")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "test_tex_format_no_truncation_markers.expected", "test_tex_format_no_truncation_markers.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "test_tex_format_no_truncation_markers.expected")?)
}

# origin: uutils test_ptx::test_truncation_no_extra_space_in_after
test test_uu_ptx_truncation_no_extra_space_in_after { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-G", "-w", "30"], stdin: b"Rust is funnnnnnnnnnnnnnnnn")?
  uu.succeeds(r1)
  uu.stdout_contains(r1, ".xx \"\" \"Rust\" \"is/\" \"\"")
}

# origin: uutils test_ptx::test_typeset_mode_default_width_100
test test_uu_ptx_typeset_mode_default_width_100 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-t"], stdin: b"bar\x0a")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "                                                     bar\n")
}

# origin: uutils test_ptx::test_typeset_mode_w_overrides_t
test test_uu_ptx_typeset_mode_w_overrides_t { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-t", "-w", "10"], stdin: b"bar\x0a")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "        bar\n")
}

# origin: uutils test_ptx::test_unicode_in_after_chunk_does_not_panic
test test_uu_ptx_unicode_in_after_chunk_does_not_panic { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", [], stdin: b"We've got +11 more G of 1.70. \xf0\x9f\x9b\xa0\x0a")?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "We've got +11")
}

# origin: uutils test_ptx::test_unicode_in_before_chunk_does_not_panic
test test_uu_ptx_unicode_in_before_chunk_does_not_panic { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-w", "10"], stdin: b"\xc3\xa9\xc3\xa9 word\x0a")?
  uu.succeeds(r1)
  uu.no_stderr(r1)
}

# origin: uutils test_ptx::test_unicode_tail_chunk_sizing
test test_uu_ptx_unicode_tail_chunk_sizing { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-w", "20"], stdin: b"a\xc3\xa9 b\xc3\xa9 KEY cc dd ee ff gg\x0a")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "test_unicode_tail_chunk_sizing.gnu.expected", "test_unicode_tail_chunk_sizing.gnu.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "test_unicode_tail_chunk_sizing.gnu.expected")?)
}

# origin: uutils test_ptx::test_unicode_truncation_alignment
test test_uu_ptx_unicode_truncation_alignment { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-w", "10"], stdin: b"f\xc3\xb6\xc3\xb6 bar")?
  uu.succeeds(r1)
  uu.fixture(s, "ptx", "test_unicode_truncation_alignment.gnu.expected", "test_unicode_truncation_alignment.gnu.expected")?
  uu.stdout_only_bytes(r1, uu.read(s, "test_unicode_truncation_alignment.gnu.expected")?)
}

# origin: uutils test_ptx::test_utf8
test test_uu_ptx_utf8 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "ptx", ["-G"], stdin: b"it\xe2\x80\x99s disabled\x0a")?
  uu.succeeds(r1)
  uu.stdout_only(r1, ".xx \"\" \"it’s\" \"disabled\" \"\"\n.xx \"\" \"\" \"it’s disabled\" \"\"\n")
  let r2 = uu.invoke(s, "ptx", ["-G", "-T"], stdin: b"it\xe2\x80\x99s disabled\x0a")?
  uu.succeeds(r2)
  uu.stdout_only(r2, "\\xx {}{it’s}{disabled}{}{}\n\\xx {}{}{it’s}{ disabled}{}\n")
}
