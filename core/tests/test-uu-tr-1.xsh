##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_tr.rs.

use support.uu as uu

# origin: uutils test_tr::alnum_expands_number_uppercase_lowercase
test test_uu_tr_alnum_expands_number_uppercase_lowercase { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[:alnum:]", " -_"], stdin: b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz")?
  uu.succeeds(r)
  uu.stdout_is(r, " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]")
}

# origin: uutils test_tr::alnum_overrides_translation_to_fallback_1
test test_uu_tr_alnum_overrides_translation_to_fallback_1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["abc[:alpha:]", "xyz"], stdin: b"abcdefghijklmnopqrstuvwxyz")?
  uu.succeeds(r)
  uu.stdout_is(r, "zzzzzzzzzzzzzzzzzzzzzzzzzz")
}

# origin: uutils test_tr::alnum_overrides_translation_to_fallback_2
test test_uu_tr_alnum_overrides_translation_to_fallback_2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[:alpha:]abc", "xyz"], stdin: b"abcdefghijklmnopqrstuvwxyz")?
  uu.succeeds(r)
  uu.stdout_is(r, "zzzzzzzzzzzzzzzzzzzzzzzzzz")
}

# origin: uutils test_tr::alpha_expands_uppercase_lowercase
test test_uu_tr_alpha_expands_uppercase_lowercase { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[:alpha:]", " -_"], stdin: b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz")?
  uu.succeeds(r)
  uu.stdout_is(r, " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRS")
}

# origin: uutils test_tr::basic_translation_works
test test_uu_tr_basic_translation_works { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["abcdef", "xyz"], stdin: b"abcdefabcdef")?
  uu.succeeds(r)
  uu.stdout_is(r, "xyzzzzxyzzzz")
}

# origin: uutils test_tr::check_class_in_set2_must_be_matched_in_set1
test test_uu_tr_check_class_in_set2_must_be_matched_in_set1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-t", "1[:upper:]", "[:upper:]"], stdin: b"")?
  uu.fails(r)
}

# origin: uutils test_tr::check_class_in_set2_must_be_matched_in_set1_right_length_check
test test_uu_tr_check_class_in_set2_must_be_matched_in_set1_right_length_check { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-t", "a-z[:upper:]", "abcdefghijklmnopqrstuvwxyz[:upper:]"], stdin: b"")?
  uu.succeeds(r)
}

# origin: uutils test_tr::check_complement_1_unique_in_set2
test test_uu_tr_check_complement_1_unique_in_set2 { |ctx|
  let s = uu.scene(ctx)?
  # Complementing uppercase leaves 230 bytes; padding determines whether y remains in string2.
  let x226 = ["x" for _ in range(226)].join("")
  let arg = x226 + "[y*]xxxx"
  let r = uu.invoke(s, "tr", ["-c", "[:upper:]", arg], stdin: b"")?
  uu.succeeds(r)
}

# origin: uutils test_tr::check_complement_2_unique_in_set2
test test_uu_tr_check_complement_2_unique_in_set2 { |ctx|
  let s = uu.scene(ctx)?
  # Complementing uppercase leaves 230 bytes; padding determines whether y remains in string2.
  let x226 = ["x" for _ in range(226)].join("")
  let arg = x226 + "[y*]xxx"
  let r = uu.invoke(s, "tr", ["-c", "[:upper:]", arg], stdin: b"")?
  uu.fails(r)
}

# origin: uutils test_tr::check_complement_set2_too_big
test test_uu_tr_check_complement_set2_too_big { |ctx|
  let s = uu.scene(ctx)?
  let x231 = ["x" for _ in range(231)].join("")
  let x230 = ["x" for _ in range(230)].join("")
  let r1 = uu.invoke(s, "tr", ["-c", "[:upper:]", x230], stdin: b"")?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "tr", ["-c", "[:upper:]", x231], stdin: b"")?
  uu.fails(r2)
  uu.stderr_contains(r2, "when translating with complemented character classes,\nstring2 must map all characters in the domain to one")
}

# origin: uutils test_tr::check_disallow_blank_in_set2_when_translating
test test_uu_tr_check_disallow_blank_in_set2_when_translating { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-t", "1234", "[:blank:]"], stdin: b"")?
  uu.fails(r)
}

# origin: uutils test_tr::check_ignore_truncate_when_deleting
test test_uu_tr_check_ignore_truncate_when_deleting { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-dt", "asdf"], stdin: b"asdfqwer\n")?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_only(r, "qwer\n")
}

# origin: uutils test_tr::check_ignore_truncate_when_deleting_and_squeezing
test test_uu_tr_check_ignore_truncate_when_deleting_and_squeezing { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-dts", "asdf", "qwe"], stdin: b"asdfqqwweerr\n")?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_only(r, "qwerr\n")
}

# origin: uutils test_tr::check_ignore_truncate_when_squeezing
test test_uu_tr_check_ignore_truncate_when_squeezing { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-ts", "asdf"], stdin: b"aassddffqwer\n")?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_only(r, "asdfqwer\n")
}

# origin: uutils test_tr::check_regression_class_blank
test test_uu_tr_check_regression_class_blank { |ctx|
  let s = uu.scene(ctx)?
  # Blank contains exactly tab and space, in this order.
  let r = uu.invoke(s, "tr", ["[:blank:][:upper:]", "12[:lower:]"], stdin: b"A\t B")?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_only(r, "a12b")
}

# origin: uutils test_tr::check_regression_class_space
test test_uu_tr_check_regression_class_space { |ctx|
  let s = uu.scene(ctx)?
  # The six spaces must expand in this order before the uppercase class.
  let r = uu.invoke(s, "tr", ["[:space:][:upper:]", "123456[:lower:]"], stdin: b"A\t\n\x0b\x0c\r B")?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_only(r, "a123456b")
}

# origin: uutils test_tr::check_regression_issue_6163_match
test test_uu_tr_check_regression_issue_6163_match { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-c", "-t", "Y", "Z"], stdin: b"\x00\n")?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_only(r, "Z\n")
}

# origin: uutils test_tr::check_regression_issue_6163_no_match
test test_uu_tr_check_regression_issue_6163_no_match { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-c", "-t", "Y", "Z"], stdin: b"X\n")?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_only(r, "X\n")
}

# origin: uutils test_tr::check_set1_longer_set2_ends_in_class
test test_uu_tr_check_set1_longer_set2_ends_in_class { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[:lower:]a", "[:upper:]"], stdin: b"")?
  uu.fails(r)
}

# origin: uutils test_tr::check_set1_longer_set2_ends_in_class_with_trunc
test test_uu_tr_check_set1_longer_set2_ends_in_class_with_trunc { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-t", "[:lower:]a", "[:upper:]"], stdin: b"")?
  uu.succeeds(r)
}

# origin: uutils test_tr::check_too_many_chars_in_eq
test test_uu_tr_check_too_many_chars_in_eq { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "[=aa=]"], stdin: b"")?
  uu.fails(r)
  uu.stderr_contains(r, "aa: equivalence class operand must be a single character\n")
}

# origin: uutils test_tr::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_tr_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  # The upstream command builder captures stderr in a regular temporary file.
  let r = uu.invoke(s, "tr", ["w[:lowre:]w", "x"], stdin: b"")?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tr: invalid character class 'lowre'\n")
}

# origin: uutils test_tr::diagnostics::test_repeat_hyphen
test test_uu_tr_diagnostics_repeat_hyphen { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "tr", ["-s", "[:blank:]", "[-*]"], stdin: b"Base Z\n")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "Base-Z\n")
  let r2 = uu.invoke(s, "tr", ["-s", "[:blank:]", "[-*5]"], stdin: b"Base Z\n")?
  uu.succeeds(r2)
  uu.stdout_only(r2, "Base-Z\n")
  let r3 = uu.invoke(s, "tr", ["[-*]", "a"], stdin: b"")?
  uu.fails_with_code(r3, 1)
  uu.stderr_only(r3, "tr: the [c*] repeat construct may not appear in string1\n")
}

# origin: uutils test_tr::missing_args_fails
test test_uu_tr_missing_args_fails { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", [], stdin: b"")?
  uu.fails(r)
  uu.stderr_contains(r, "missing operand")
}

# origin: uutils test_tr::missing_required_second_arg_fails
test test_uu_tr_missing_required_second_arg_fails { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["foo"], stdin: b"")?
  uu.fails(r)
  uu.stderr_contains(r, "missing operand after")
}

# origin: uutils test_tr::non_octal_repeat_count_test
test test_uu_tr_non_octal_repeat_count_test { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["ABCdefghijkl", "[x*10]Y"], stdin: b"ABCdefghijklmn12")?
  uu.succeeds(r)
  uu.stdout_is(r, "xxxxxxxxxxYYmn12")
}

# origin: uutils test_tr::octal_repeat_count_test
test test_uu_tr_octal_repeat_count_test { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["ABCdefghijkl", "[x*010]Y"], stdin: b"ABCdefghijklmn12")?
  uu.succeeds(r)
  uu.stdout_is(r, "xxxxxxxxYYYYmn12")
}

# origin: uutils test_tr::overrides_translation_pair_if_repeats
test test_uu_tr_overrides_translation_pair_if_repeats { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["aaa", "xyz"], stdin: b"aaa")?
  uu.succeeds(r)
  uu.stdout_is(r, "zzz")
}

# origin: uutils test_tr::test_backwards_range
test test_uu_tr_backwards_range { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "\\046-\\048"], stdin: b"")?
  uu.fails(r)
  uu.stderr_only(r, "tr: range-endpoints of '&-\\004' are in reverse collating sequence order\n")
}

# origin: uutils test_tr::test_complement1
test test_uu_tr_complement1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-c", "a", "X"], stdin: b"ab")?
  uu.succeeds(r)
  uu.stdout_is(r, "aX")
}

# origin: uutils test_tr::test_complement2
test test_uu_tr_complement2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-c", "0-9", "x"], stdin: b"Phone: 01234 567890")?
  uu.succeeds(r)
  uu.stdout_is(r, "xxxxxxx01234x567890")
}

# origin: uutils test_tr::test_complement3
test test_uu_tr_complement3 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-c", "abcdefgh", "123"], stdin: b"the cat and the bat")?
  uu.succeeds(r)
  uu.stdout_is(r, "3he3ca33a3d33he3ba3")
}

# origin: uutils test_tr::test_complement4
test test_uu_tr_complement4 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-c", "0-@", "*-~"], stdin: b"0x1y2z3")?
  uu.succeeds(r)
  uu.stdout_is(r, "0~1~2~3")
}

# origin: uutils test_tr::test_complement5
test test_uu_tr_complement5 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-c", "\\0-@", "*-~"], stdin: b"0x1y2z3")?
  uu.succeeds(r)
  uu.stdout_is(r, "0a1b2c3")
}

# origin: uutils test_tr::test_complement_afterwards_is_not_flag
test test_uu_tr_complement_afterwards_is_not_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["a", "X", "-c"], stdin: b"")?
  uu.fails(r)
  uu.stderr_contains(r, "extra operand '-c'")
}

# origin: uutils test_tr::test_complement_flag_fails_with_more_than_two_operand
test test_uu_tr_complement_flag_fails_with_more_than_two_operand { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-c", "a", "b", "c"], stdin: b"")?
  uu.fails(r)
  uu.stderr_contains(r, "extra operand 'c'")
}

# origin: uutils test_tr::test_complement_multi_early
test test_uu_tr_complement_multi_early { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-c", "-c", "a", "X"], stdin: b"ab")?
  uu.succeeds(r)
  uu.stdout_is(r, "aX")
}

# origin: uutils test_tr::test_complement_multi_late
test test_uu_tr_complement_multi_late { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-c", "a", "X", "-c"], stdin: b"")?
  uu.fails(r)
  uu.stderr_contains(r, "tr: extra operand '-c'")
}

# origin: uutils test_tr::test_complement_multi_middle
test test_uu_tr_complement_multi_middle { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-c", "a", "-c", "X"], stdin: b"")?
  uu.fails(r)
  uu.stderr_contains(r, "tr: extra operand 'X'")
}

# origin: uutils test_tr::test_delete
test test_uu_tr_delete { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "a-z"], stdin: b"aBcD")?
  uu.succeeds(r)
  uu.stdout_is(r, "BD")
}

# origin: uutils test_tr::test_delete_afterwards_is_not_flag
test test_uu_tr_delete_afterwards_is_not_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["a-z", "-d"], stdin: b"aBcD")?
  uu.succeeds(r)
  uu.stdout_is(r, "-BdD")
}

# origin: uutils test_tr::test_delete_and_squeeze
test test_uu_tr_delete_and_squeeze { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-ds", "a-z", "A-Z"], stdin: b"abBcB")?
  uu.succeeds(r)
  uu.stdout_is(r, "B")
}

# origin: uutils test_tr::test_delete_and_squeeze_complement
test test_uu_tr_delete_and_squeeze_complement { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-dsc", "a-z", "A-Z"], stdin: b"abBcB")?
  uu.succeeds(r)
  uu.stdout_is(r, "abc")
}

# origin: uutils test_tr::test_delete_and_squeeze_complement_squeeze_set2
test test_uu_tr_delete_and_squeeze_complement_squeeze_set2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-dsc", "abX", "XYZ"], stdin: b"abbbcdddXXXYYY")?
  uu.succeeds(r)
  uu.stdout_is(r, "abbbX")
}

# origin: uutils test_tr::test_delete_and_squeeze_one_set
test test_uu_tr_delete_and_squeeze_one_set { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-ds", "a-z"], stdin: b"")?
  uu.fails(r)
  uu.stderr_contains(r, "missing operand after 'a-z'")
  uu.stderr_contains(r, "Two strings must be given when both deleting and squeezing repeats.")
}

# origin: uutils test_tr::test_delete_complement
test test_uu_tr_delete_complement { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "-c", "a-z"], stdin: b"aBcD")?
  uu.succeeds(r)
  uu.stdout_is(r, "ac")
}

# origin: uutils test_tr::test_delete_complement_2
test test_uu_tr_delete_complement_2 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "tr", ["-d", "-C", "0-9"], stdin: b"Phone: 01234 567890")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "01234567890")
  let r2 = uu.invoke(s, "tr", ["-d", "--complement", "0-9"], stdin: b"Phone: 01234 567890")?
  uu.succeeds(r2)
  uu.stdout_is(r2, "01234567890")
}

# origin: uutils test_tr::test_delete_complement_graph_and_print_match_gnu
test test_uu_tr_delete_complement_graph_and_print_match_gnu { |ctx|
  let s = uu.scene(ctx)?
  let input = b" A!\t\n"
  let r1 = uu.invoke(s, "tr", ["-d", "-c", "[:graph:]"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b"A!")
  let r2 = uu.invoke(s, "tr", ["-d", "-c", "[:print:]"], stdin: input)?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, b" A!")
}

# origin: uutils test_tr::test_delete_flag_takes_only_one_operand
test test_uu_tr_delete_flag_takes_only_one_operand { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "a", "p"], stdin: b"")?
  uu.fails(r)
  uu.stderr_contains(r, "extra operand 'p'\nOnly one string may be given when deleting without squeezing repeats.")
}

# origin: uutils test_tr::test_delete_graph_and_print_match_gnu
test test_uu_tr_delete_graph_and_print_match_gnu { |ctx|
  let s = uu.scene(ctx)?
  let input = b" A!\t\n"
  let r1 = uu.invoke(s, "tr", ["-d", "[:graph:]"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b" \t\n")
  let r2 = uu.invoke(s, "tr", ["-d", "[:print:]"], stdin: input)?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, b"\t\n")
}

# origin: uutils test_tr::test_delete_late
test test_uu_tr_delete_late { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "a-z", "-d"], stdin: b"")?
  uu.fails(r)
  uu.stderr_contains(r, "extra operand '-d'")
}

# origin: uutils test_tr::test_delete_multi
test test_uu_tr_delete_multi { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "-d", "a-z"], stdin: b"aBcD")?
  uu.succeeds(r)
  uu.stdout_is(r, "BD")
}

# origin: uutils test_tr::test_failed_write_is_reported
test test_uu_tr_failed_write_is_reported { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["e", "a"], stdin: b"hello", stdout: /dev/full)?
  uu.fails(r)
  uu.stderr_is(r, "tr: write error: No space left on device\n")
}

# origin: uutils test_tr::test_huge_repeat_count_in_set1
test test_uu_tr_huge_repeat_count_in_set1 { |ctx|
  let s = uu.scene(ctx)?
  # Huge counts must stay compact rather than expanding before stdin is read.
  let r1 = uu.invoke(s, "tr", ["[a*9223372036854775808]", "b"], stdin: b"abc")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "bbc")
  let r2 = uu.invoke(s, "tr", ["[a*99999999999999]b", "xy"], stdin: b"abc")?
  uu.succeeds(r2)
  uu.stdout_only(r2, "yyc")
  let r3 = uu.invoke(s, "tr", ["-t", "[a*99999999999999]", "x"], stdin: b"abc")?
  uu.succeeds(r3)
  uu.stdout_only(r3, "xbc")
  let r4 = uu.invoke(s, "tr", ["-d", "[a*99999999999999]"], stdin: b"abc")?
  uu.succeeds(r4)
  uu.stdout_only(r4, "bc")
}

# origin: uutils test_tr::test_huge_repeat_count_in_set2
test test_uu_tr_huge_repeat_count_in_set2 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "tr", ["abc", "[x*99999999999999]"], stdin: b"abc")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "xxx")
  let r2 = uu.invoke(s, "tr", ["abcd", "[x*99999999999999]yz"], stdin: b"abcd")?
  uu.succeeds(r2)
  uu.stdout_only(r2, "xxxx")
  let r3 = uu.invoke(s, "tr", ["-c", "a", "[x*99999999999999]"], stdin: b"abc")?
  uu.succeeds(r3)
  uu.stdout_only(r3, "axx")
}

# origin: uutils test_tr::test_interpret_backslash_at_eol_literally
test test_uu_tr_interpret_backslash_at_eol_literally { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["X", "\\"], stdin: b"X")?
  uu.succeeds(r)
  uu.stdout_is(r, "\\")
}

# origin: uutils test_tr::test_interpret_backslash_escapes
test test_uu_tr_interpret_backslash_escapes { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["abfnrtv", "\\a\\b\\f\\n\\r\\t\\v"], stdin: b"abfnrtv")?
  uu.succeeds(r)
  uu.stdout_is(r, "\x07\x08\x0c\n\r\t\x0b")
}

# origin: uutils test_tr::test_interpret_one_and_two_digit_octal_escape
test test_uu_tr_interpret_one_and_two_digit_octal_escape { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["XYZ", "\\0\\11\\77"], stdin: b"XYZ")?
  uu.succeeds(r)
  uu.stdout_is(r, "\x00\t?")
}

# origin: uutils test_tr::test_interpret_single_octal_escape
test test_uu_tr_interpret_single_octal_escape { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["X", "\\015"], stdin: b"X")?
  uu.succeeds(r)
  uu.stdout_is(r, "\r")
}

# origin: uutils test_tr::test_interpret_unrecognized_backslash_escape_as_character
test test_uu_tr_interpret_unrecognized_backslash_escape_as_character { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["qcz+=~-", "\\q\\c\\z\\+\\=\\~\\-"], stdin: b"qcz+=~-")?
  uu.succeeds(r)
  uu.stdout_is(r, "qcz+=~-")
}

# origin: uutils test_tr::test_invalid_arg
test test_uu_tr_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["--definitely-invalid"], stdin: b"")?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_tr::test_invalid_input
test test_uu_tr_invalid_input { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["1", "1", "<", "."])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "tr: extra operand '<'")
  let directory = uu.invoke_from_path(s, "tr", ["1", "1"], s.root)?
  uu.fails_with_code(directory, 1)
  uu.stderr_contains(directory, "tr: read error: Is a directory")
}

# origin: uutils test_tr::test_invalid_unicode
test test_uu_tr_invalid_unicode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-dc", "abc"], stdin: b"\x80abc")?
  uu.succeeds(r)
  uu.stdout_is(r, "abc")
}

# origin: uutils test_tr::test_more_than_2_sets
test test_uu_tr_more_than_2_sets { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["'abcdef'", "'a'", "'b'"], stdin: b"")?
  uu.fails(r)
}

# origin: uutils test_tr::test_multibyte_octal_sequence
test test_uu_tr_multibyte_octal_sequence { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-d", "\\501"], stdin: bytes.from_text("(1Ł)"))?
  uu.succeeds(r)
  uu.stderr_is(r, "tr: warning: the ambiguous octal escape \\501 is being\n\tinterpreted as the 2-byte sequence \\050, 1\n")
  uu.stdout_is(r, "Ł)")
}

# origin: uutils test_tr::test_non_digit_repeat
test test_uu_tr_non_digit_repeat { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["a", "[b*c]"], stdin: b"")?
  uu.fails(r)
  uu.stderr_only(r, "tr: invalid repeat count 'c' in [c*n] construct\n")
}

# origin: uutils test_tr::test_non_octal_digit_ends_escape
test test_uu_tr_non_octal_digit_ends_escape { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["rust", "\\08\\11956"], stdin: b"rust")?
  uu.succeeds(r)
  uu.stdout_is(r, "\x008\t9")
}

# origin: uutils test_tr::test_octal_escape_is_at_most_three_digits
test test_uu_tr_octal_escape_is_at_most_three_digits { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["XY", "\\0156"], stdin: b"XY")?
  uu.succeeds(r)
  uu.stdout_is(r, "\r6")
}

# origin: uutils test_tr::test_octal_warning_still_fires_after_a_bad_sequence
test test_uu_tr_octal_warning_still_fires_after_a_bad_sequence { |ctx|
  let s = uu.scene(ctx)?
  # Parse the whole set so the octal warning follows an earlier invalid class.
  let r = uu.invoke(s, "tr", ["[:foo:]\\400", "y"], stdin: b"")?
  uu.fails(r)
  uu.stderr_is(r, "tr: warning: the ambiguous octal escape \\400 is being\n\tinterpreted as the 2-byte sequence \\040, 0\ntr: invalid character class 'foo'\n")
}

# origin: uutils test_tr::test_repeat_in_set1_padded_by_star_in_set2
test test_uu_tr_repeat_in_set1_padded_by_star_in_set2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[a*3]bc", "x[y*]z"], stdin: b"abc")?
  uu.succeeds(r)
  uu.stdout_only(r, "yyz")
}

# origin: uutils test_tr::test_repeat_keeps_every_set2_character_for_squeeze
test test_uu_tr_repeat_keeps_every_set2_character_for_squeeze { |ctx|
  let s = uu.scene(ctx)?
  # The last mapping wins, but every string2 byte still belongs to the squeeze set.
  let r1 = uu.invoke(s, "tr", ["-s", "[a*2]", "xy"], stdin: b"xxaa")?
  uu.succeeds(r1)
  uu.stdout_only(r1, "xy")
  let r2 = uu.invoke(s, "tr", ["-s", "a", "xyz"], stdin: b"aazz")?
  uu.succeeds(r2)
  uu.stdout_only(r2, "xz")
}

# origin: uutils test_tr::test_set1_longer_than_set2
test test_uu_tr_set1_longer_than_set2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["abc", "xy"], stdin: b"abcde")?
  uu.succeeds(r)
  uu.stdout_is(r, "xyyde")
}

# origin: uutils test_tr::test_set1_shorter_than_set2
test test_uu_tr_set1_shorter_than_set2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["ab", "xyz"], stdin: b"abcde")?
  uu.succeeds(r)
  uu.stdout_is(r, "xycde")
}

# origin: uutils test_tr::test_small_set2
test test_uu_tr_small_set2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["0-9", "X"], stdin: b"@0123456789")?
  uu.succeeds(r)
  uu.stdout_is(r, "@XXXXXXXXXX")
}

# origin: uutils test_tr::test_squeeze
test test_uu_tr_squeeze { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-s", "a-z"], stdin: b"aaBBcDcc")?
  uu.succeeds(r)
  uu.stdout_is(r, "aBBcDc")
}

# origin: uutils test_tr::test_squeeze_complement
test test_uu_tr_squeeze_complement { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-sc", "a-z"], stdin: b"aaBBcDcc")?
  uu.succeeds(r)
  uu.stdout_is(r, "aaBcDcc")
}

# origin: uutils test_tr::test_squeeze_complement_multi
test test_uu_tr_squeeze_complement_multi { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-scsc", "a-z"], stdin: b"aaBBcDcc")?
  uu.succeeds(r)
  uu.stdout_is(r, "aaBcDcc")
}

# origin: uutils test_tr::test_squeeze_complement_two_sets
test test_uu_tr_squeeze_complement_two_sets { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-sc", "a", "_"], stdin: b"test a aa with 3 ___ spaaaces +++")?
  uu.succeeds(r)
  uu.stdout_is(r, "_a_aa_aaa_")
}

# origin: uutils test_tr::test_squeeze_flag_fails_with_more_than_two_operand
test test_uu_tr_squeeze_flag_fails_with_more_than_two_operand { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-s", "a", "b", "c"], stdin: b"")?
  uu.fails(r)
  uu.stderr_contains(r, "extra operand 'c'")
}
