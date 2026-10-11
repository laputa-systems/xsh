##! Transcribed from the MIT-licensed uutils printf integration tests.
use support.uu as uu

# origin: uutils test_printf::basic_literal
test test_uu_printf_basic_literal { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["hello world"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "hello world")
}

# origin: uutils test_printf::char_as_byte
test test_uu_printf_char_as_byte { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%c", "🙃"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, b"\xf0")
}

# origin: uutils test_printf::char_constant_warning_preserves_prior_failure
test test_uu_printf_char_constant_warning_preserves_prior_failure { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%d\n", "bad", "'ab"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_is(r1, "0\n97\n")
  uu.stderr_is(r1, "printf: 'bad': expected a numeric value\nprintf: warning: b: character(s) following character constant have been ignored\n")
  let r2 = uu.invoke(s, "printf", ["%d\n", "bad", "'ab"], vars: {POSIXLY_CORRECT: "1"})?
  uu.fails_with_code(r2, 1)
  uu.stdout_is(r2, "0\n97\n")
  uu.stderr_is(r2, "printf: 'bad': expected a numeric value\n")
}

# origin: uutils test_printf::diagnostics::test_piped_stderr_keeps_the_plain_message
test test_uu_printf_diagnostics_piped_stderr_keeps_the_plain_message { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%5.2c", "q"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "printf: %5.2c: invalid conversion specification\n")
}

# origin: uutils test_printf::double_dash_after_the_format_is_an_argument
test test_uu_printf_double_dash_after_the_format_is_an_argument { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%s\n", "--"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "--\n")
  let r2 = uu.invoke(s, "printf", ["%s %s\n", "--", "--"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "-- --\n")
  let r3 = uu.invoke(s, "printf", ["--", "%s\n", "--"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "--\n")
}

# origin: uutils test_printf::double_dash_as_the_format_is_printed_literally
test test_uu_printf_double_dash_as_the_format_is_printed_literally { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["--", "--", "x"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "--")
  uu.stderr_contains(r1, "warning: ignoring excess arguments, starting with 'x'")
}

# origin: uutils test_printf::escaped_octal_and_newline
test test_uu_printf_escaped_octal_and_newline { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["\\101\\0377\\n"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "A\x1f7\n")
}

# origin: uutils test_printf::escaped_percent_sign
test test_uu_printf_escaped_percent_sign { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["hello%% world"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "hello% world")
}

# origin: uutils test_printf::escaped_unicode_eight_digit
test test_uu_printf_escaped_unicode_eight_digit { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["\\U00000125"])?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b"\x5cu0125")
  uu.stderr_is_bytes(r1, b"")
}

# origin: uutils test_printf::escaped_unicode_four_digit
test test_uu_printf_escaped_unicode_four_digit { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["\\u0125"])?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b"\x5cu0125")
  uu.stderr_is_bytes(r1, b"")
}

# origin: uutils test_printf::escaped_unicode_incomplete
test test_uu_printf_escaped_unicode_incomplete { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["\\u"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "printf: missing hexadecimal number in escape\n")
  let r2 = uu.invoke(s, "printf", ["\\U"])?
  uu.fails_with_code(r2, 1)
  uu.stderr_only(r2, "printf: missing hexadecimal number in escape\n")
  let r3 = uu.invoke(s, "printf", ["\\uabc"])?
  uu.fails_with_code(r3, 1)
  uu.stderr_only(r3, "printf: missing hexadecimal number in escape\n")
  let r4 = uu.invoke(s, "printf", ["\\Uabcd"])?
  uu.fails_with_code(r4, 1)
  uu.stderr_only(r4, "printf: missing hexadecimal number in escape\n")
}

# origin: uutils test_printf::escaped_unicode_invalid
test test_uu_printf_escaped_unicode_invalid { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["\\ud9d0"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_is_bytes(r1, b"")
  uu.stderr_is_bytes(r1, b"printf: invalid universal character name \x5cud9d0\x0a")
  let r2 = uu.invoke(s, "printf", ["\\U0000D8F9"])?
  uu.fails_with_code(r2, 1)
  uu.stdout_is_bytes(r2, b"")
  uu.stderr_is_bytes(r2, b"printf: invalid universal character name \x5cU0000d8f9\x0a")
}

# origin: uutils test_printf::escaped_unicode_null_byte
test test_uu_printf_escaped_unicode_null_byte { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["\\0001_"])?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b"\x001_")
  let r2 = uu.invoke(s, "printf", ["%b", "\\0001_"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, b"\x01_")
}

# origin: uutils test_printf::escaped_unrecognized
test test_uu_printf_escaped_unrecognized { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["c\\d"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "c\\d")
}

# origin: uutils test_printf::flag_position_space_padding
test test_uu_printf_flag_position_space_padding { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["% +3.1d", "1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, " +1")
}

# origin: uutils test_printf::float_abs_value_less_than_one
test test_uu_printf_float_abs_value_less_than_one { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%g", "0.1171875"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.117188")
  let r2 = uu.invoke(s, "printf", ["%g", "-0.1171875"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "-0.117188")
  let r3 = uu.invoke(s, "printf", ["%g", "0.01171875"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "0.0117188")
  let r4 = uu.invoke(s, "printf", ["%g", "-0.01171875"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "-0.0117188")
  let r5 = uu.invoke(s, "printf", ["%g", "0.001171875001"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "0.00117188")
  let r6 = uu.invoke(s, "printf", ["%g", "-0.001171875001"])?
  uu.succeeds(r6)
  uu.stdout_only(r6, "-0.00117188")
}

# origin: uutils test_printf::float_arg_invalid
test test_uu_printf_float_arg_invalid { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%f", "."])?
  uu.fails(r1)
  uu.stdout_is(r1, "0.000000")
  uu.stderr_contains(r1, "expected a numeric value")
  let r2 = uu.invoke(s, "printf", ["%f", "-."])?
  uu.fails(r2)
  uu.stdout_is(r2, "0.000000")
  uu.stderr_contains(r2, "expected a numeric value")
  let r3 = uu.invoke(s, "printf", ["%f", "e"])?
  uu.fails(r3)
  uu.stdout_is(r3, "0.000000")
  uu.stderr_contains(r3, "expected a numeric value")
  let r4 = uu.invoke(s, "printf", ["%f", ".e12"])?
  uu.fails(r4)
  uu.stdout_is(r4, "0.000000")
  uu.stderr_contains(r4, "expected a numeric value")
  let r5 = uu.invoke(s, "printf", ["%f", "123e"])?
  uu.fails(r5)
  uu.stdout_is(r5, "123.000000")
  uu.stderr_contains(r5, "value not completely converted")
  let r6 = uu.invoke(s, "printf", ["%f", "0x"])?
  uu.fails(r6)
  uu.stdout_is(r6, "0.000000")
  uu.stderr_contains(r6, "value not completely converted")
  let r7 = uu.invoke(s, "printf", ["%f", "0x."])?
  uu.fails(r7)
  uu.stdout_is(r7, "0.000000")
  uu.stderr_contains(r7, "value not completely converted")
  let r8 = uu.invoke(s, "printf", ["%f", "0xp12"])?
  uu.fails(r8)
  uu.stdout_is(r8, "0.000000")
  uu.stderr_contains(r8, "value not completely converted")
}

# origin: uutils test_printf::float_arg_with_whitespace
test test_uu_printf_float_arg_with_whitespace { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%f", "  \r\t\n0.000001"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.000001")
  let r2 = uu.invoke(s, "printf", ["%f", "0.1 "])?
  uu.fails(r2)
  uu.stderr_contains(r2, "value not completely converted")
  let r3 = uu.invoke(s, "printf", ["%f", " 0.1"])?
  uu.fails(r3)
  uu.stderr_contains(r3, "expected a numeric value")
  let r4 = uu.invoke(s, "printf", ["%f", "\\t0.1"])?
  uu.fails(r4)
  uu.stderr_contains(r4, "expected a numeric value")
}

# origin: uutils test_printf::float_arg_zero
test test_uu_printf_float_arg_zero { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%f", "0."])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.000000")
  let r2 = uu.invoke(s, "printf", ["%f", ".0"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "0.000000")
  let r3 = uu.invoke(s, "printf", ["%f", ".0e100000"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "0.000000")
}

# origin: uutils test_printf::float_default_precision_space_padding
test test_uu_printf_float_default_precision_space_padding { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%10f", "1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "  1.000000")
}

# origin: uutils test_printf::float_default_precision_zero_padding
test test_uu_printf_float_default_precision_zero_padding { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%010f", "1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "001.000000")
}

# origin: uutils test_printf::float_flag_position_space_padding
test test_uu_printf_float_flag_position_space_padding { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["% +5.1f", "1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, " +1.0")
}

# origin: uutils test_printf::float_large_precision
test test_uu_printf_float_large_precision { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%.30f", "0.1"])?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b"0.100000000000000000001355252716")
  uu.stderr_is_bytes(r1, b"")
}

# origin: uutils test_printf::float_non_finite
test test_uu_printf_float_non_finite { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%f %f %F %f %f %F", "nan", "-nan", "nan", "inf", "-inf", "inf"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "nan -nan NAN inf -inf INF")
}

# origin: uutils test_printf::float_non_finite_space_padding
test test_uu_printf_float_non_finite_space_padding { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["% 5.2f|% 5.2f|% 5.2f|% 5.2f", "inf", "-inf", "nan", "-nan"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "  inf| -inf|  nan| -nan")
}

# origin: uutils test_printf::float_non_finite_zero_padding
test test_uu_printf_float_non_finite_zero_padding { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%05.2f|%05.2f|%05.2f|%05.2f", "inf", "-inf", "nan", "-nan"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "  inf| -inf|  nan| -nan")
}

# origin: uutils test_printf::float_space_padding_with_precision
test test_uu_printf_float_space_padding_with_precision { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%4.1f", "1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, " 1.0")
}

# origin: uutils test_printf::float_switch_switch_decimal_scientific
test test_uu_printf_float_switch_switch_decimal_scientific { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%g", "0.0001"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.0001")
  let r2 = uu.invoke(s, "printf", ["%g", "0.00001"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "1e-05")
}

# origin: uutils test_printf::float_with_zero_precision_should_pad
test test_uu_printf_float_with_zero_precision_should_pad { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%03.0f", "-1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-01")
}

# origin: uutils test_printf::float_zero_neg_zero
test test_uu_printf_float_zero_neg_zero { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%f %f", "0.0", "-0.0"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.000000 -0.000000")
}

# origin: uutils test_printf::float_zero_padding_with_precision
test test_uu_printf_float_zero_padding_with_precision { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%04.1f", "1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "01.0")
}

# origin: uutils test_printf::format_spec_zero_fails
test test_uu_printf_format_spec_zero_fails { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%0c", "3"])?
  uu.fails_with_code(r1, 1)
  let r2 = uu.invoke(s, "printf", ["%0s", "3"])?
  uu.fails_with_code(r2, 1)
}

# origin: uutils test_printf::help_and_version_past_the_format_are_arguments
test test_uu_printf_help_and_version_past_the_format_are_arguments { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%s", "--help"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "--help")
  let r2 = uu.invoke(s, "printf", ["%s", "--version"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "--version")
  let r3 = uu.invoke(s, "printf", ["--", "--help"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "--help")
}

# origin: uutils test_printf::int_with_zero_precision_and_zero_value
test test_uu_printf_int_with_zero_precision_and_zero_value { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%.0d", "0"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "")
  let r2 = uu.invoke(s, "printf", ["%#.0o", "0"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "0")
  let r3 = uu.invoke(s, "printf", ["%.*d", "-1", "0"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "0")
}

# origin: uutils test_printf::invalid_precision_tests
test test_uu_printf_invalid_precision_tests { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%.*d", "2147483648", "0"])?
  uu.fails(r1)
  uu.stderr_is(r1, "printf: invalid precision: '2147483648'\n")
  let r2 = uu.invoke(s, "printf", ["%.*f", "2147483648", "0"])?
  uu.fails(r2)
  uu.stderr_is(r2, "printf: invalid precision: '2147483648'\n")
}

# origin: uutils test_printf::leading_double_dash_ends_the_options
test test_uu_printf_leading_double_dash_ends_the_options { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["--", "%s\n", "a"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "a\n")
}

# origin: uutils test_printf::negative_float_zero_padding_with_precision
test test_uu_printf_negative_float_zero_padding_with_precision { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%05.1f", "-1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-01.0")
}

# origin: uutils test_printf::negative_zero_padding_test
test test_uu_printf_negative_zero_padding_test { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%03d", "-1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-01")
}

# origin: uutils test_printf::negative_zero_padding_with_space_test
test test_uu_printf_negative_zero_padding_with_space_test { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["% 03d", "-1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-01")
}

# origin: uutils test_printf::no_infinite_loop
test test_uu_printf_no_infinite_loop { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["a", "b"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "a")
  uu.stderr_contains(r1, "warning: ignoring excess arguments, starting with 'b'")
}

# origin: uutils test_printf::pad_char
test test_uu_printf_pad_char { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%3c", "X"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "  X")
  let r2 = uu.invoke(s, "printf", ["%1c", "X"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "X")
  let r3 = uu.invoke(s, "printf", ["%-1c", "X"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "X")
  let r4 = uu.invoke(s, "printf", ["%-3c", "X"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "X  ")
}

# origin: uutils test_printf::pad_octal_with_prefix
test test_uu_printf_pad_octal_with_prefix { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", [">%#15.6o<", "0"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, ">         000000<")
  let r2 = uu.invoke(s, "printf", [">%#15.6o<", "01"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, ">         000001<")
  let r3 = uu.invoke(s, "printf", [">%#15.6o<", "01234"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, ">         001234<")
  let r4 = uu.invoke(s, "printf", [">%#15.6o<", "012345"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, ">         012345<")
  let r5 = uu.invoke(s, "printf", [">%#15.6o<", "0123456"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, ">        0123456<")
}

# origin: uutils test_printf::pad_string
test test_uu_printf_pad_string { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%8s", "bottle"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "  bottle")
  let r2 = uu.invoke(s, "printf", ["%-8s", "bottle"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "bottle  ")
  let r3 = uu.invoke(s, "printf", ["%6s", "bottle"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "bottle")
  let r4 = uu.invoke(s, "printf", ["%-6s", "bottle"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "bottle")
}

# origin: uutils test_printf::pad_unsigned_three
test test_uu_printf_pad_unsigned_three { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%.3u", "3"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "003")
  let r2 = uu.invoke(s, "printf", ["%.3x", "3"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "003")
  let r3 = uu.invoke(s, "printf", ["%.3X", "3"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "003")
  let r4 = uu.invoke(s, "printf", ["%.3o", "3"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "003")
  let r5 = uu.invoke(s, "printf", ["%#.3x", "3"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "0x003")
  let r6 = uu.invoke(s, "printf", ["%#.3X", "3"])?
  uu.succeeds(r6)
  uu.stdout_only(r6, "0X003")
  let r7 = uu.invoke(s, "printf", ["%#.3o", "3"])?
  uu.succeeds(r7)
  uu.stdout_only(r7, "003")
  let r8 = uu.invoke(s, "printf", ["%#05x", "3"])?
  uu.succeeds(r8)
  uu.stdout_only(r8, "0x003")
  let r9 = uu.invoke(s, "printf", ["%#05X", "3"])?
  uu.succeeds(r9)
  uu.stdout_only(r9, "0X003")
  let r10 = uu.invoke(s, "printf", ["%3x", "3"])?
  uu.succeeds(r10)
  uu.stdout_only(r10, "  3")
  let r11 = uu.invoke(s, "printf", ["%3X", "3"])?
  uu.succeeds(r11)
  uu.stdout_only(r11, "  3")
}

# origin: uutils test_printf::pad_unsigned_zeroes
test test_uu_printf_pad_unsigned_zeroes { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%.3u", "0"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "000")
  let r2 = uu.invoke(s, "printf", ["%.3x", "0"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "000")
  let r3 = uu.invoke(s, "printf", ["%.3X", "0"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "000")
  let r4 = uu.invoke(s, "printf", ["%.3o", "0"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "000")
}

# origin: uutils test_printf::partial_char
test test_uu_printf_partial_char { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%d", "'abc"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "97")
  uu.stderr_is(r1, "printf: warning: bc: character(s) following character constant have been ignored\n")
}

# origin: uutils test_printf::partial_char_posixly_correct
test test_uu_printf_partial_char_posixly_correct { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%d", "'AB"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  let r2 = uu.invoke(s, "printf", ["%d", "'ABC"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r2)
  uu.no_stderr(r2)
  let r3 = uu.invoke(s, "printf", ["%d", "\"AB"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r3)
  uu.no_stderr(r3)
  let r4 = uu.invoke(s, "printf", ["%d", "'-1"], vars: {POSIXLY_CORRECT: "1"})?
  uu.succeeds(r4)
  uu.no_stderr(r4)
  let r5 = uu.invoke(s, "printf", ["%d", "'AB"], vars: {POSIXLY_CORRECT: ""})?
  uu.succeeds(r5)
  uu.no_stderr(r5)
  let r6 = uu.invoke(s, "printf", ["%d", "'ABC"], vars: {POSIXLY_CORRECT: ""})?
  uu.succeeds(r6)
  uu.no_stderr(r6)
  let r7 = uu.invoke(s, "printf", ["%d", "\"AB"], vars: {POSIXLY_CORRECT: ""})?
  uu.succeeds(r7)
  uu.no_stderr(r7)
  let r8 = uu.invoke(s, "printf", ["%d", "'-1"], vars: {POSIXLY_CORRECT: ""})?
  uu.succeeds(r8)
  uu.no_stderr(r8)
  let r9 = uu.invoke(s, "printf", ["%d", "'AB"], vars: {POSIXLY_CORRECT: "0"})?
  uu.succeeds(r9)
  uu.no_stderr(r9)
  let r10 = uu.invoke(s, "printf", ["%d", "'ABC"], vars: {POSIXLY_CORRECT: "0"})?
  uu.succeeds(r10)
  uu.no_stderr(r10)
  let r11 = uu.invoke(s, "printf", ["%d", "\"AB"], vars: {POSIXLY_CORRECT: "0"})?
  uu.succeeds(r11)
  uu.no_stderr(r11)
  let r12 = uu.invoke(s, "printf", ["%d", "'-1"], vars: {POSIXLY_CORRECT: "0"})?
  uu.succeeds(r12)
  uu.no_stderr(r12)
  let r13 = uu.invoke(s, "printf", ["%d", "'AB"])?
  uu.succeeds(r13)
  uu.stderr_is(r13, "printf: warning: B: character(s) following character constant have been ignored\n")
  let r14 = uu.invoke(s, "printf", ["%d", "'ABC"])?
  uu.succeeds(r14)
  uu.stderr_is(r14, "printf: warning: BC: character(s) following character constant have been ignored\n")
  let r15 = uu.invoke(s, "printf", ["%d", "\"AB"])?
  uu.succeeds(r15)
  uu.stderr_is(r15, "printf: warning: B: character(s) following character constant have been ignored\n")
  let r16 = uu.invoke(s, "printf", ["%d", "'-1"])?
  uu.succeeds(r16)
  uu.stderr_is(r16, "printf: warning: 1: character(s) following character constant have been ignored\n")
}

# origin: uutils test_printf::partial_float
test test_uu_printf_partial_float { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%.2f is %s", "42.03x", "a lot"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_is(r1, "42.03 is a lot")
  uu.stderr_is(r1, "printf: '42.03x': value not completely converted\n")
}

# origin: uutils test_printf::partial_integer
test test_uu_printf_partial_integer { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%d is %s", "42x23", "a lot"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_is(r1, "42 is a lot")
  uu.stderr_is(r1, "printf: '42x23': value not completely converted\n")
  let r2 = uu.invoke(s, "printf", ["%d is not %s", "0xwa", "a lot"])?
  uu.fails_with_code(r2, 1)
  uu.stdout_is(r2, "0 is not a lot")
  uu.stderr_is(r2, "printf: '0xwa': value not completely converted\n")
}

# origin: uutils test_printf::positional_format_specifiers
test test_uu_printf_positional_format_specifiers { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%1$d%d-", "5", "10", "6", "20"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "55-1010-66-2020-")
  let r2 = uu.invoke(s, "printf", ["%2$d%d-", "5", "10", "6", "20"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "105-206-")
  let r3 = uu.invoke(s, "printf", ["%3$d%d-", "5", "10", "6", "20"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "65-020-")
  let r4 = uu.invoke(s, "printf", ["%4$d%d-", "5", "10", "6", "20"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "205-")
  let r5 = uu.invoke(s, "printf", ["%5$d%d-", "5", "10", "6", "20"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "05-")
  let r6 = uu.invoke(s, "printf", ["%0$d%d-", "5", "10", "6", "20"])?
  uu.fails_with_code(r6, 1)
  uu.stderr_only(r6, "printf: %0$: invalid conversion specification\n")
  let r7 = uu.invoke(s, "printf", ["Octal: %6$o, Int: %1$d, Float: %4$f, String: %2$s, Hex: %7$x, Scientific: %5$e, Char: %9$c, Unsigned: %3$u, Integer: %8$i", "42", "hello", "100", "3.14159", "0.00001", "77", "255", "123", "A"])?
  uu.succeeds(r7)
  uu.stdout_only(r7, "Octal: 115, Int: 42, Float: 3.141590, String: hello, Hex: ff, Scientific: 1.000000e-05, Char: A, Unsigned: 100, Integer: 123")
}

# origin: uutils test_printf::precision_check
test test_uu_printf_precision_check { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%.3d", "1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "001")
}

# origin: uutils test_printf::space_padding_with_precision
test test_uu_printf_space_padding_with_precision { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%4.3d", "1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, " 001")
}

# origin: uutils test_printf::space_padding_with_space_test
test test_uu_printf_space_padding_with_space_test { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["% 3d", "1"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "  1")
}

# origin: uutils test_printf::spaces_before_numbers_are_ignored
test test_uu_printf_spaces_before_numbers_are_ignored { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%*.*d", "   5", "  3", " 6"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "  006")
}

# origin: uutils test_printf::stop_after_additional_escape
test test_uu_printf_stop_after_additional_escape { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["A%sC\\cD%sF", "B", "E"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "ABC")
}

# origin: uutils test_printf::stop_after_additional_escape_in_b_string
test test_uu_printf_stop_after_additional_escape_in_b_string { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["A%bB\\n", "x\\cy"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "Ax")
}

# origin: uutils test_printf::sub_alternative_lower_hex
test test_uu_printf_sub_alternative_lower_hex { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%#x", "42"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0x2a")
}

# origin: uutils test_printf::sub_alternative_lower_hex_0
test test_uu_printf_sub_alternative_lower_hex_0 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%#x", "0"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0")
}

# origin: uutils test_printf::sub_alternative_upper_hex
test test_uu_printf_sub_alternative_upper_hex { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%#X", "42"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0X2A")
}

# origin: uutils test_printf::sub_alternative_upper_hex_0
test test_uu_printf_sub_alternative_upper_hex_0 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%#X", "0"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0")
}

# origin: uutils test_printf::sub_any_asterisk_both_params
test test_uu_printf_sub_any_asterisk_both_params { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%*.*i", "4", "3", "11", "5", "4", "12"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, " 011 0012")
}

# origin: uutils test_printf::sub_any_asterisk_first_param
test test_uu_printf_sub_any_asterisk_first_param { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%*i", "3", "11", "4", "12"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, " 11  12")
}

# origin: uutils test_printf::sub_any_asterisk_first_param_with_integer
test test_uu_printf_sub_any_asterisk_first_param_with_integer { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["|%*d|", "3", "0"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "|  0|")
  let r2 = uu.invoke(s, "printf", ["|%*d|", "1", "0"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "|0|")
  let r3 = uu.invoke(s, "printf", ["|%*d|", "0", "0"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "|0|")
  let r4 = uu.invoke(s, "printf", ["|%*d|", "-1", "0"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "|0|")
  let r5 = uu.invoke(s, "printf", ["|%*d|", "-3", "0"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "|0  |")
}

# origin: uutils test_printf::sub_any_asterisk_hex_arg
test test_uu_printf_sub_any_asterisk_hex_arg { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%.*i", "0xA", "123456789"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0123456789")
}

# origin: uutils test_printf::sub_any_asterisk_negative_first_param
test test_uu_printf_sub_any_asterisk_negative_first_param { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["a(%*s)b", "-5", "xyz"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "a(xyz  )b")
  let r2 = uu.invoke(s, "printf", ["a(%*s)b", "-010", "xyz"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "a(xyz     )b")
  let r3 = uu.invoke(s, "printf", ["a(%*s)b", "-0x10", "xyz"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "a(xyz             )b")
  let r4 = uu.invoke(s, "printf", ["a(%*c)b", "-5", "x"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "a(x    )b")
}

# origin: uutils test_printf::sub_any_asterisk_octal_arg
test test_uu_printf_sub_any_asterisk_octal_arg { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%.*i", "011", "12345678"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "012345678")
}

# origin: uutils test_printf::sub_any_asterisk_second_param
test test_uu_printf_sub_any_asterisk_second_param { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%.*i", "3", "11", "4", "12"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0110012")
}

# origin: uutils test_printf::sub_any_asterisk_second_param_with_integer
test test_uu_printf_sub_any_asterisk_second_param_with_integer { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["|%.*d|", "3", "10"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "|010|")
  let r2 = uu.invoke(s, "printf", ["|%*.d|", "1", "10"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "|10|")
  let r3 = uu.invoke(s, "printf", ["|%.*d|", "0", "10"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "|10|")
  let r4 = uu.invoke(s, "printf", ["|%.*d|", "-1", "10"])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "|10|")
  let r5 = uu.invoke(s, "printf", ["|%.*d|", "-2", "10"])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "|10|")
  let r6 = uu.invoke(s, "printf", ["|%.*d|", "-9223372036854775808", "10"])?
  uu.succeeds(r6)
  uu.stdout_only(r6, "|10|")
  let r7 = uu.invoke(s, "printf", ["|%.*d|", "-340282366920938463463374607431768211455", "10"])?
  uu.fails_with_code(r7, 1)
  uu.stdout_is(r7, "|10|")
  uu.stderr_is(r7, "printf: '-340282366920938463463374607431768211455': Numerical result out of range\n")
}

# origin: uutils test_printf::sub_any_specifiers
test test_uu_printf_sub_any_specifiers { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%ztlhLji", "3"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "3")
  let r2 = uu.invoke(s, "printf", ["%0ztlhLji", "3"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "3")
  let r3 = uu.invoke(s, "printf", ["%0.ztlhLji", "3"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "3")
}

# origin: uutils test_printf::sub_any_specifiers_after_second_param
test test_uu_printf_sub_any_specifiers_after_second_param { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%0.0ztlhLji", "3"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "3")
}

# origin: uutils test_printf::sub_b_string_handle_escapes
test test_uu_printf_sub_b_string_handle_escapes { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["hello %b", "\\tworld"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "hello \tworld")
}

# origin: uutils test_printf::sub_b_string_ignore_subs
test test_uu_printf_sub_b_string_ignore_subs { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["hello %b", "world %% %i"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "hello world %% %i")
}

# origin: uutils test_printf::sub_b_string_validate_field_params
test test_uu_printf_sub_b_string_validate_field_params { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["hello %7b", "world"])?
  uu.fails(r1)
  uu.stdout_is(r1, "hello ")
  uu.stderr_is(r1, "printf: %7b: invalid conversion specification\n")
}

# origin: uutils test_printf::sub_b_string_variable_size_unicode
test test_uu_printf_sub_b_string_variable_size_unicode { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["|%b", "\\5|"])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, b"|\x05|")
  let r2 = uu.invoke(s, "printf", ["|%b", "\\05|"])?
  uu.succeeds(r2)
  uu.stdout_only_bytes(r2, b"|\x05|")
  let r3 = uu.invoke(s, "printf", ["|%b", "\\005|"])?
  uu.succeeds(r3)
  uu.stdout_only_bytes(r3, b"|\x05|")
  let r4 = uu.invoke(s, "printf", ["|%b", "\\0005|"])?
  uu.succeeds(r4)
  uu.stdout_only_bytes(r4, b"|\x05|")
  let r5 = uu.invoke(s, "printf", ["|%b", "\\00005|"])?
  uu.succeeds(r5)
  uu.stdout_only_bytes(r5, b"|\x005|")
}

# origin: uutils test_printf::sub_char
test test_uu_printf_sub_char { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["the letter %c", "A"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "the letter A")
}

# origin: uutils test_printf::sub_char_from_string
test test_uu_printf_sub_char_from_string { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%c%c%c", "five", "%", "oval"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "f%o")
}

# origin: uutils test_printf::sub_float_dec_places
test test_uu_printf_sub_float_dec_places { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["pi is ~ %.11f", "3.1415926535"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "pi is ~ 3.14159265350")
}

# origin: uutils test_printf::mb_input
test test_uu_printf_mb_input { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "printf", ["%04x\n", "\"á"])?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, b"00c3\x0a")
  uu.stderr_is_bytes(r1, b"printf: warning: \xa1: character(s) following character constant have been ignored\x0a")
  let r2 = uu.invoke(s, "printf", ["%04x\n", "'á"])?
  uu.succeeds(r2)
  uu.stdout_is_bytes(r2, b"00c3\x0a")
  uu.stderr_is_bytes(r2, b"printf: warning: \xa1: character(s) following character constant have been ignored\x0a")
  let r3 = uu.invoke(s, "printf", ["%04x\n", "'á"])?
  uu.succeeds(r3)
  uu.stdout_is_bytes(r3, b"00c3\x0a")
  uu.stderr_is_bytes(r3, b"printf: warning: \xa1: character(s) following character constant have been ignored\x0a")
  let r4 = uu.invoke(s, "printf", ["%i\n", "\"á"])?
  uu.succeeds(r4)
  uu.stdout_is_bytes(r4, b"195\x0a")
  uu.stderr_is_bytes(r4, b"printf: warning: \xa1: character(s) following character constant have been ignored\x0a")
  let r5 = uu.invoke(s, "printf", ["%i\n", "'á"])?
  uu.succeeds(r5)
  uu.stdout_is_bytes(r5, b"195\x0a")
  uu.stderr_is_bytes(r5, b"printf: warning: \xa1: character(s) following character constant have been ignored\x0a")
  let r6 = uu.invoke(s, "printf", ["%i\n", "'á"])?
  uu.succeeds(r6)
  uu.stdout_is_bytes(r6, b"195\x0a")
  uu.stderr_is_bytes(r6, b"printf: warning: \xa1: character(s) following character constant have been ignored\x0a")
  let r7 = uu.invoke(s, "printf", ["%f\n", "'á"])?
  uu.succeeds(r7)
  uu.stdout_is_bytes(r7, b"195.000000\x0a")
  uu.stderr_is_bytes(r7, b"printf: warning: \xa1: character(s) following character constant have been ignored\x0a")
  let r8 = uu.invoke(s, "printf", ["%04x\n", "\"á="])?
  uu.succeeds(r8)
  uu.stdout_is_bytes(r8, b"00c3\x0a")
  uu.stderr_is_bytes(r8, b"printf: warning: \xa1=: character(s) following character constant have been ignored\x0a")
  let r9 = uu.invoke(s, "printf", ["%04x\n", "'á-"])?
  uu.succeeds(r9)
  uu.stdout_is_bytes(r9, b"00c3\x0a")
  uu.stderr_is_bytes(r9, b"printf: warning: \xa1-: character(s) following character constant have been ignored\x0a")
  let r10 = uu.invoke(s, "printf", ["%04x\n", "'á=-=="])?
  uu.succeeds(r10)
  uu.stdout_is_bytes(r10, b"00c3\x0a")
  uu.stderr_is_bytes(r10, b"printf: warning: \xa1=-==: character(s) following character constant have been ignored\x0a")
  let r11 = uu.invoke(s, "printf", ["%04x\n", "'á'"])?
  uu.succeeds(r11)
  uu.stdout_is_bytes(r11, b"00c3\x0a")
  uu.stderr_is_bytes(r11, b"printf: warning: \xa1': character(s) following character constant have been ignored\x0a")
  let r12 = uu.invoke(s, "printf", ["%04x\n", "'á++"])?
  uu.succeeds(r12)
  uu.stdout_is_bytes(r12, b"00c3\x0a")
  uu.stderr_is_bytes(r12, b"printf: warning: \xa1++: character(s) following character constant have been ignored\x0a")
  let r13 = uu.invoke(s, "printf", ["%04x\n", "''á'"])?
  uu.succeeds(r13)
  uu.stdout_is_bytes(r13, b"0027\x0a")
  uu.stderr_is_bytes(r13, b"printf: warning: \xc3\xa1': character(s) following character constant have been ignored\x0a")
  let r14 = uu.invoke(s, "printf", ["%i\n", "\"á="])?
  uu.succeeds(r14)
  uu.stdout_is_bytes(r14, b"195\x0a")
  uu.stderr_is_bytes(r14, b"printf: warning: \xa1=: character(s) following character constant have been ignored\x0a")
  let r15 = uu.invoke(s, "printf", ["%04x\n", "\""])?
  uu.fails_with_code(r15, 1)
  uu.stdout_is_bytes(r15, b"0000\x0a")
  uu.stderr_is_bytes(r15, b"printf: '\x22': expected a numeric value\x0a")
  let r16 = uu.invoke(s, "printf", ["%04x\n", "'"])?
  uu.fails_with_code(r16, 1)
  uu.stdout_is_bytes(r16, b"0000\x0a")
  uu.stderr_is_bytes(r16, b"printf: '\x5c'': expected a numeric value\x0a")
}

# origin: uutils test_printf::mb_invalid_unicode
test test_uu_printf_mb_invalid_unicode { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke_paths(s, "printf", [Path("%04x\n"), Path.parse_bytes(b"\x22\xe1")?])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "00e1\n")
  let r2 = uu.invoke_paths(s, "printf", [Path("%04x\n"), Path.parse_bytes(b"'\xe1")?])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "00e1\n")
  let r3 = uu.invoke_paths(s, "printf", [Path("%i\n"), Path.parse_bytes(b"\x22\xe1")?])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "225\n")
  let r4 = uu.invoke_paths(s, "printf", [Path("%i\n"), Path.parse_bytes(b"'\xe1")?])?
  uu.succeeds(r4)
  uu.stdout_only(r4, "225\n")
  let r5 = uu.invoke_paths(s, "printf", [Path("%f\n"), Path.parse_bytes(b"'\xe1")?])?
  uu.succeeds(r5)
  uu.stdout_only(r5, "225.000000\n")
  let r6 = uu.invoke_paths(s, "printf", [Path("%04x\n"), Path.parse_bytes(b"\x22\xe1=")?])?
  uu.succeeds(r6)
  uu.stdout_is(r6, "00e1\n")
  uu.stderr_is(r6, "printf: warning: =: character(s) following character constant have been ignored\n")
  let r7 = uu.invoke_paths(s, "printf", [Path("%04x\n"), Path.parse_bytes(b"'\xe1-")?])?
  uu.succeeds(r7)
  uu.stdout_is(r7, "00e1\n")
  uu.stderr_is(r7, "printf: warning: -: character(s) following character constant have been ignored\n")
  let r8 = uu.invoke_paths(s, "printf", [Path("%04x\n"), Path.parse_bytes(b"'\xe1=-==")?])?
  uu.succeeds(r8)
  uu.stdout_is(r8, "00e1\n")
  uu.stderr_is(r8, "printf: warning: =-==: character(s) following character constant have been ignored\n")
  let r9 = uu.invoke_paths(s, "printf", [Path("%04x\n"), Path.parse_bytes(b"'\xe1'")?])?
  uu.succeeds(r9)
  uu.stdout_is(r9, "00e1\n")
  uu.stderr_is(r9, "printf: warning: ': character(s) following character constant have been ignored\n")
}

# origin: uutils test_printf::non_utf_8_input
test test_uu_printf_non_utf_8_input { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke_paths(s, "printf", [Path("%s"), Path.parse_bytes(b"Swer an rehte g\xfcete wendet s\xeen gem\xfcete, dem volget s\xe6lde und \xeare.")?])?
  uu.succeeds(r1)
  uu.stdout_only_bytes(r1, b"Swer an rehte g\xfcete wendet s\xeen gem\xfcete, dem volget s\xe6lde und \xeare.")
  let r2 = uu.invoke_paths(s, "printf", [Path.parse_bytes(b"Swer an rehte g\xfcete wendet s\xeen gem\xfcete, dem volget s\xe6lde und \xeare.")?])?
  uu.succeeds(r2)
  uu.stdout_only_bytes(r2, b"Swer an rehte g\xfcete wendet s\xeen gem\xfcete, dem volget s\xe6lde und \xeare.")
  let r3 = uu.invoke_paths(s, "printf", [Path("%d"), Path.parse_bytes(b"Swer an rehte g\xfcete wendet s\xeen gem\xfcete, dem volget s\xe6lde und \xeare.")?])?
  uu.fails(r3)
  assert bytes.from_text("expected a numeric value") in r3.stderr
}
