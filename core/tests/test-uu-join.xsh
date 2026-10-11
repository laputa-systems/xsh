##! Native join compatibility tests transcribed from the uutils integration suite.

use support.uu as uu

proc fixtures(s: uu.Scene) {
  for name in [
    "autoformat.expected",
    "capitalized.txt",
    "case_insensitive.expected",
    "contiguous_separators.txt",
    "default.expected",
    "different_field.expected",
    "different_fields.expected",
    "different_lengths.txt",
    "empty.txt",
    "empty_key.expected",
    "fields_1.txt",
    "fields_2.txt",
    "fields_3.txt",
    "fields_4.txt",
    "fields_5.txt",
    "header.expected",
    "header_1.txt",
    "header_2.txt",
    "header_autoformat.expected",
    "missing_format_fields.expected",
    "multibyte_sep.expected",
    "multibyte_sep_1.txt",
    "multibyte_sep_2.txt",
    "non-line_feeds.expected",
    "non-line_feeds_1.txt",
    "non-line_feeds_2.txt",
    "non-unicode.expected",
    "non-unicode_1.bin",
    "non-unicode_2.bin",
    "non-unicode_sep.expected",
    "null-sep.expected",
    "out_of_bounds_fields.expected",
    "semicolon_fields_1.txt",
    "semicolon_fields_2.txt",
    "semicolon_separated.expected",
    "suppress_joined.expected",
    "suppress_joined_outer.expected",
    "unpaired_lines.expected",
    "unpaired_lines_format.expected",
    "unpaired_lines_outer.expected",
    "z.expected",
  ] {
    uu.fixture(s, "join", name, name)?
  }
}

# origin: uutils test_join::autoformat
test test_uu_join_autoformat { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_2.txt", "different_lengths.txt", "-o", "auto"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "autoformat.expected")?)
  let r2 = uu.invoke(s, "join", ["-", "fields_2.txt", "-o", "auto"], stdin: bytes.from_text("1 x y z\n2 p"))?
  uu.succeeds(r2)
  uu.stdout_only(r2, "1 x y z a\n2 p   b\n")
  let r3 = uu.invoke(s, "join", ["-", "fields_2.txt", "-a", "1", "-o", "auto", "-e", "."], stdin: bytes.from_text("1 x y z\n2 p\n99 a b\n"))?
  uu.succeeds(r3)
  uu.stdout_only(r3, "1 x y z a\n2 p . . b\n99 a b . .\n")
}

# origin: uutils test_join::case_insensitive
test test_uu_join_case_insensitive { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["capitalized.txt", "fields_3.txt", "-i"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "case_insensitive.expected")?)
}

# origin: uutils test_join::default_arguments
test test_uu_join_default_arguments { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_1.txt", "fields_2.txt"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "default.expected")?)
}

# origin: uutils test_join::default_format
test test_uu_join_default_format { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_1.txt", "fields_2.txt", "-o", "1.1 2.2"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "default.expected")?)
  let r2 = uu.invoke(s, "join", ["fields_1.txt", "fields_2.txt", "-o", "0 2.2"])?
  uu.succeeds(r2)
  uu.stdout_only_bytes(r2, uu.read(s, "default.expected")?)
}

# origin: uutils test_join::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_join_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["-o", "1.2,2.x", "/dev/null", "/dev/null"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_is(r1, "join: invalid field number: 'x'\n")
}

# origin: uutils test_join::different_field
test test_uu_join_different_field { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_2.txt", "fields_3.txt", "-2", "2"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "different_field.expected")?)
}

# origin: uutils test_join::different_fields
test test_uu_join_different_fields { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_2.txt", "fields_4.txt", "-j", "2"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "different_fields.expected")?)
  let r2 = uu.invoke(s, "join", ["fields_2.txt", "fields_4.txt", "-1", "2", "-2", "2"])?
  uu.succeeds(r2)
  uu.stdout_only_bytes(r2, uu.read(s, "different_fields.expected")?)
}

# origin: uutils test_join::empty_fields_kept_without_empty_filler
test test_uu_join_empty_fields_kept_without_empty_filler { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  uu.write(s, "gap", "a,,b\n")?
  let r1 = uu.invoke(s, "join", ["-t", ",", "-o", "0,1.1,1.2,1.3", "gap", "gap"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "a,a,,b\n")
  let r2 = uu.invoke(s, "join", ["-t", ",", "-e", "", "-o", "0,1.1,1.2,1.3", "gap", "gap"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "a,a,,b\n")
}

# origin: uutils test_join::empty_fields_use_empty_filler
test test_uu_join_empty_fields_use_empty_filler { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  uu.write(s, "blank", "hello\n\n\n")?
  let r1 = uu.invoke(s, "join", ["-e", "EMPTY", "blank", "blank"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "hello\nEMPTY\nEMPTY\nEMPTY\nEMPTY\n")
  uu.write(s, "spaces", "   \n")?
  let r2 = uu.invoke(s, "join", ["-e", "EMPTY", "-o", "0,1.1", "spaces", "spaces"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "EMPTY EMPTY\n")
  uu.write(s, "gap", "a,,b\n")?
  let r3 = uu.invoke(s, "join", ["-t", ",", "-e", "EMPTY", "-o", "0,1.1,1.2,1.3", "gap", "gap"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "a,a,EMPTY,b\n")
  let r4 = uu.invoke(s, "join", ["-t", ",", "-e", "EMPTY", "gap", "gap"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "a,EMPTY,b,EMPTY,b\n")
}

# origin: uutils test_join::empty_files
test test_uu_join_empty_files { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["empty.txt", "empty.txt"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "")
  let r2 = uu.invoke(s, "join", ["empty.txt", "fields_1.txt"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "")
  let r3 = uu.invoke(s, "join", ["fields_1.txt", "empty.txt"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "")
}

# origin: uutils test_join::empty_format
test test_uu_join_empty_format { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_1.txt", "fields_2.txt", "-o", ""])?
  uu.fails(r1)
  uu.stderr_is(r1, "join: invalid file number in field spec: ''\n")
}

# origin: uutils test_join::empty_intersection
test test_uu_join_empty_intersection { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_1.txt", "fields_2.txt", "-2", "2"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "")
}

# origin: uutils test_join::empty_key
test test_uu_join_empty_key { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_1.txt", "empty.txt", "-j", "2", "-a", "1", "-e", "x"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "empty_key.expected")?)
}

# origin: uutils test_join::headers
test test_uu_join_headers { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["header_1.txt", "header_2.txt", "--header"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "header.expected")?)
}

# origin: uutils test_join::headers_autoformat
test test_uu_join_headers_autoformat { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["header_1.txt", "header_2.txt", "--header", "-o", "auto"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "header_autoformat.expected")?)
}

# origin: uutils test_join::join_emoji_delim_inner_key
test test_uu_join_join_emoji_delim_inner_key { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  uu.write(s, "file1", "a🗿b\n")?
  uu.write(s, "file2", "u🗿b\n")?
  let r1 = uu.invoke(s, "join", ["-t🗿", "-1", "2", "-2", "2", "file1", "file2"], vars: {LC_ALL: "C.utf8"})?
  uu.succeeds(r1)
  uu.stdout_only(r1, "b🗿a🗿u\n")
}

# origin: uutils test_join::missing_format_fields
test test_uu_join_missing_format_fields { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_2.txt", "different_lengths.txt", "-o", "0 1.2 2.4", "-e", "x"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "missing_format_fields.expected")?)
}

# origin: uutils test_join::multibyte_sep
test test_uu_join_multibyte_sep { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["-t§", "multibyte_sep_1.txt", "multibyte_sep_2.txt"], vars: {LC_ALL: "C.utf8"})?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "multibyte_sep.expected")?)
}

# origin: uutils test_join::new_line_separated
test test_uu_join_new_line_separated { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["-", "fields_2.txt", "-t", ""], stdin: bytes.from_text("1 a\n1 b\n8 h\n"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1 a\n8 h\n")
}

# origin: uutils test_join::nocheck_order
test test_uu_join_nocheck_order { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_1.txt", "fields_2.txt", "--nocheck-order"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "default.expected")?)
}

# origin: uutils test_join::non_line_feeds
test test_uu_join_non_line_feeds { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["non-line_feeds_1.txt", "non-line_feeds_2.txt"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "non-line_feeds.expected")?)
}

# origin: uutils test_join::null_field_separators
test test_uu_join_null_field_separators { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["-t", "\\0", "non-unicode_1.bin", "non-unicode_2.bin"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "null-sep.expected")?)
}

# origin: uutils test_join::null_line_endings
test test_uu_join_null_line_endings { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["-z", "non-unicode_1.bin", "non-unicode_2.bin"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "z.expected")?)
}

# origin: uutils test_join::only_whitespace_separators_merge
test test_uu_join_only_whitespace_separators_merge { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["contiguous_separators.txt", "-"], stdin: bytes.from_text(" a  ,c "))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "a ,,,b ,c\n")
  let r2 = uu.invoke(s, "join", ["contiguous_separators.txt", "-t", ",", "-"], stdin: bytes.from_text(" a  ,c "))?
  uu.succeeds(r2)
  uu.stdout_only(r2, " a  ,,,b,c \n")
}

# origin: uutils test_join::out_of_bounds_fields
test test_uu_join_out_of_bounds_fields { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_1.txt", "fields_4.txt", "-1", "3", "-2", "5"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "out_of_bounds_fields.expected")?)
  let r2 = uu.invoke(s, "join", ["fields_1.txt", "fields_4.txt", "-j", "100000000000000000000"])?
  uu.succeeds(r2)
  uu.stdout_only_bytes(r2, uu.read(s, "out_of_bounds_fields.expected")?)
}

# origin: uutils test_join::repeated_o_accumulates_fields
test test_uu_join_repeated_o_accumulates_fields { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_1.txt", "fields_2.txt", "-o", "2.2", "-o", "1.1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "a 1\nb 2\nc 3\ne 5\nh 8\n")
}

# origin: uutils test_join::repeated_o_auto_stays_auto
test test_uu_join_repeated_o_auto_stays_auto { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_1.txt", "fields_2.txt", "-o", "auto", "-o", "auto"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "default.expected")?)
}

# origin: uutils test_join::repeated_o_ignores_auto_when_mixed
test test_uu_join_repeated_o_ignores_auto_when_mixed { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_1.txt", "fields_2.txt", "-o", "auto", "-o", "2.2 2.2 1.1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "a a 1\nb b 2\nc c 3\ne e 5\nh h 8\n")
}

# origin: uutils test_join::semicolon_separated
test test_uu_join_semicolon_separated { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["semicolon_fields_1.txt", "semicolon_fields_2.txt", "-t", ";"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "semicolon_separated.expected")?)
}

# origin: uutils test_join::single_file_with_header
test test_uu_join_single_file_with_header { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["capitalized.txt", "empty.txt", "--header"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "A 1\n")
  let r2 = uu.invoke(s, "join", ["empty.txt", "capitalized.txt", "--header"])?
  uu.succeeds(r2)
  uu.stdout_is(r2, "A 1\n")
}

# origin: uutils test_join::suppress_joined
test test_uu_join_suppress_joined { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_3.txt", "fields_2.txt", "-1", "2", "-v", "2"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "suppress_joined.expected")?)
  let r2 = uu.invoke(s, "join", ["fields_3.txt", "fields_2.txt", "-1", "2", "-a", "1", "-v", "2"])?
  uu.succeeds(r2)
  uu.stdout_only_bytes(r2, uu.read(s, "suppress_joined_outer.expected")?)
}

# origin: uutils test_join::tab_hyphen_leading_as_separate_arg
test test_uu_join_tab_hyphen_leading_as_separate_arg { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["semicolon_fields_1.txt", "semicolon_fields_2.txt", "-t", "-x"])?
  uu.fails(r1)
  uu.stderr_is(r1, "join: multi-character tab '-x'\n")
}

# origin: uutils test_join::tab_multi_character
test test_uu_join_tab_multi_character { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["semicolon_fields_1.txt", "semicolon_fields_2.txt", "-t", "ab"])?
  uu.fails(r1)
  uu.stderr_is(r1, "join: multi-character tab 'ab'\n")
}

# origin: uutils test_join::test_full
test test_uu_join_full { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_1.txt", "fields_2.txt"], stdout: p"/dev/full")?
  uu.fails(r1)
  uu.stderr_contains(r1, "No space left on device")
}

# origin: uutils test_join::test_invalid_arg
test test_uu_join_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["--definitely-invalid"])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_join::test_locale_collation
test test_uu_join_locale_collation { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  uu.write(s, "f1.sorted", "abc:d 2\nab:d  1\n")?
  uu.write(s, "f2.sorted", "abc:d y\nab:d  x\n")?
  let r1 = uu.invoke(s, "join", ["--check-order", "f1.sorted", "f2.sorted"], vars: {LC_ALL: "en_US.UTF-8"})?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "abc:d 2 y")
  uu.stdout_contains(r1, "ab:d 1 x")
}

# origin: uutils test_join::unpaired_lines
test test_uu_join_unpaired_lines { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_2.txt", "fields_3.txt", "-a", "1"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "fields_2.txt")?)
  let r2 = uu.invoke(s, "join", ["fields_3.txt", "fields_2.txt", "-1", "2", "-a", "2"])?
  uu.succeeds(r2)
  uu.stdout_only_bytes(r2, uu.read(s, "unpaired_lines.expected")?)
  let r3 = uu.invoke(s, "join", ["fields_3.txt", "fields_2.txt", "-1", "2", "-a", "1", "-a", "2"])?
  uu.succeeds(r3)
  uu.stdout_only_bytes(r3, uu.read(s, "unpaired_lines_outer.expected")?)
}

# origin: uutils test_join::unpaired_lines_format
test test_uu_join_unpaired_lines_format { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_2.txt", "fields_3.txt", "-a", "2", "-o", "1.2 1.1 2.4 2.3 2.2 0"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "unpaired_lines_format.expected")?)
}

# origin: uutils test_join::non_unicode
test test_uu_join_non_unicode { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["non-unicode_1.bin", "non-unicode_2.bin"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, uu.read(s, "non-unicode.expected")?)
  let r2 = uu.invoke_paths(s, "join", [p"-t", Path.parse_bytes(b"\xa7")?, p"non-unicode_1.bin", p"non-unicode_2.bin"])?
  uu.succeeds(r2)
  uu.stdout_only_bytes(r2, uu.read(s, "non-unicode_sep.expected")?)
  let r3 = uu.invoke_paths(s, "join", [p"-t", Path.parse_bytes(b"\xa7\xa7")?, p"non-unicode_1.bin", p"non-unicode_2.bin"])?
  uu.fails(r3)
  # The invalid separator is quoted losslessly in GNU diagnostics.
  uu.stderr_is(r3, "join: multi-character tab '\\247\\247'\n")
}

# origin: uutils test_join::test_join_non_utf8_paths
test test_uu_join_join_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let first = uu.at_bytes(s, b"test_\xff\xfe_1.txt")?
  let second = uu.at_bytes(s, b"test_\xff\xfe_2.txt")?
  first.write(b"a 1\n")?
  second.write(b"a 2\n")?
  let r1 = uu.invoke_paths(s, "join", [Path.parse_bytes(b"test_\xff\xfe_1.txt")?, Path.parse_bytes(b"test_\xff\xfe_2.txt")?])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "a 1 2\n")
}

# origin: uutils test_join::test_incompatible_fields_reports_exact_field_number
test test_uu_join_incompatible_fields_reports_exact_field_number { |ctx|
  let s = uu.scene(ctx)?
  # GNU reports the stored zero-based fields and clamps at the signed integer ceiling.
  for fields in [
    {field: "3", expected: "2"},
    {field: "18446744073709551615", expected: "9223372036854775806"},
    {field: "99999999999999999999999", expected: "9223372036854775806"},
    {field: "9007199254740993", expected: "9007199254740992"},
  ] {
    let r1 = uu.invoke(s, "join", ["-j", fields.field, "-1", "5", "/dev/null", "/dev/null"])?
    uu.fails(r1)
    uu.stderr_contains(r1, f"incompatible join fields {fields.expected}, 4")
  }
}

# origin: uutils test_join::test_hyphen_leading_field_number_is_reported_as_invalid
test test_uu_join_hyphen_leading_field_number_is_reported_as_invalid { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  for opt in ["-1", "-2", "-j"] {
    let r1 = uu.invoke(s, "join", [opt, "-1", "empty.txt", "empty.txt"])?
    uu.fails(r1)
    uu.stderr_contains(r1, "invalid field number: '-1'")
  }
}

# origin: uutils test_join::wrong_line_order
test test_uu_join_wrong_line_order { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_2.txt", "fields_4.txt"])?
  uu.fails(r1)
  uu.stdout_contains(r1, "7 g f 4 fg")
  uu.stderr_is(r1, "join: fields_4.txt:5: is not sorted: 11 g 5 gh\njoin: input is not in sorted order\n")
  let r2 = uu.invoke(s, "join", ["--check-order", "fields_2.txt", "fields_4.txt"])?
  uu.fails(r2)
  assert "7 g f 4 fg" not in r2.stdout.utf8()?
  uu.stderr_is(r2, "join: fields_4.txt:5: is not sorted: 11 g 5 gh\n")
}

# origin: uutils test_join::both_files_wrong_line_order
test test_uu_join_both_files_wrong_line_order { |ctx|
  let s = uu.scene(ctx)?
  fixtures(s)
  let r1 = uu.invoke(s, "join", ["fields_4.txt", "fields_5.txt"])?
  uu.fails(r1)
  uu.stdout_contains(r1, "5 e 3 ef")
  uu.stderr_is(r1, "join: fields_5.txt:4: is not sorted: 3\njoin: fields_4.txt:5: is not sorted: 11 g 5 gh\njoin: input is not in sorted order\n")
  let r2 = uu.invoke(s, "join", ["--check-order", "fields_4.txt", "fields_5.txt"])?
  uu.fails(r2)
  assert "5 e 3 ef" not in r2.stdout.utf8()?
  uu.stderr_is(r2, "join: fields_5.txt:4: is not sorted: 3\n")
}
