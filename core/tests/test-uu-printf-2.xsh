##! Native ports of the uutils printf integration tests.

use support.uu as uu

# origin: uutils test_printf::sub_float_hex_in
test test_uu_printf_sub_float_hex_in { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%f", "0xF1.1F"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "241.121094")
}

# origin: uutils test_printf::sub_float_leading_zeroes
test test_uu_printf_sub_float_leading_zeroes { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%010f", "1"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "001.000000")
}

# origin: uutils test_printf::sub_float_no_octal_in
test test_uu_printf_sub_float_no_octal_in { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%f", "077"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "77.000000")
}

# origin: uutils test_printf::sub_general_float
test test_uu_printf_sub_general_float { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%g", "1.1"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1.1")
}

# origin: uutils test_printf::sub_general_round_float
test test_uu_printf_sub_general_round_float { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%g", "12345.6789"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "12345.7")
}

# origin: uutils test_printf::sub_general_round_float_leading_zeroes
test test_uu_printf_sub_general_round_float_leading_zeroes { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%g", "1.000009"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1.00001")
}

# origin: uutils test_printf::sub_general_round_float_to_integer
test test_uu_printf_sub_general_round_float_to_integer { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%g", "123456.7"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "123457")
}

# origin: uutils test_printf::sub_general_round_scientific_notation
test test_uu_printf_sub_general_round_scientific_notation { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%g", "123456789"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1.23457e+08")
}

# origin: uutils test_printf::sub_general_scientific_notation
test test_uu_printf_sub_general_scientific_notation { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%g", "1000010"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1.00001e+06")
}

# origin: uutils test_printf::sub_general_truncate_to_integer
test test_uu_printf_sub_general_truncate_to_integer { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%g", "1.0"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1")
}

# origin: uutils test_printf::sub_int_decimal
test test_uu_printf_sub_int_decimal { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%0.i", "11"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "11")
}

# origin: uutils test_printf::sub_int_leading_zeroes
test test_uu_printf_sub_int_leading_zeroes { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%.4i", "11"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "0011")
}

# origin: uutils test_printf::sub_int_leading_zeroes_padded
test test_uu_printf_sub_int_leading_zeroes_padded { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%5.4i", "11"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, " 0011")
}

# origin: uutils test_printf::sub_min_width
test test_uu_printf_sub_min_width { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["hello %7s", "world"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "hello   world")
}

# origin: uutils test_printf::sub_min_width_negative
test test_uu_printf_sub_min_width_negative { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["hello %-7s", "world"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "hello world  ")
}

# origin: uutils test_printf::sub_num_dec_trunc
test test_uu_printf_sub_num_dec_trunc { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["pi is ~ %g", "3.1415926535"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "pi is ~ 3.14159")
}

# origin: uutils test_printf::sub_num_float
test test_uu_printf_sub_num_float { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["twenty is %f", "20"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "twenty is 20.000000")
}

# origin: uutils test_printf::sub_num_float_e_no_round
test test_uu_printf_sub_num_float_e_no_round { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%e", "99999994"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "9.999999e+07")
}

# origin: uutils test_printf::sub_num_float_e_round
test test_uu_printf_sub_num_float_e_round { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%e", "99999999"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1.000000e+08")
}

# origin: uutils test_printf::sub_num_float_round_nines_dec
test test_uu_printf_sub_num_float_round_nines_dec { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%f", "0.99999999"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "1.000000")
}

# origin: uutils test_printf::sub_num_float_round_to_one
test test_uu_printf_sub_num_float_round_to_one { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["one is %f", "0.9999995"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "one is 0.999999")
}

# origin: uutils test_printf::sub_num_hex_float_lower
test test_uu_printf_sub_num_hex_float_lower { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%a", ".875"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "0xep-4")
}

# origin: uutils test_printf::sub_num_hex_float_upper
test test_uu_printf_sub_num_hex_float_upper { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%A", ".875"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "0XEP-4")
}

# origin: uutils test_printf::sub_num_hex_lower
test test_uu_printf_sub_num_hex_lower { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["thirty in hex is %x", "30"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "thirty in hex is 1e")
}

# origin: uutils test_printf::sub_num_hex_non_numerical
test test_uu_printf_sub_num_hex_non_numerical { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["parameters need to be numbers %X", "%194"])?
  uu.fails_with_code(r0, 1)
}

# origin: uutils test_printf::sub_num_hex_upper
test test_uu_printf_sub_num_hex_upper { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["thirty in hex is %X", "30"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "thirty in hex is 1E")
}

# origin: uutils test_printf::sub_num_int
test test_uu_printf_sub_num_int { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["twenty is %i", "20"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "twenty is 20")
}

# origin: uutils test_printf::sub_num_int_char_const_in
test test_uu_printf_sub_num_int_char_const_in { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["ninety seven is %i", "'a"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "ninety seven is 97")
  let r1 = uu.invoke(s, "printf", ["emoji is %i", "'🙃"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "emoji is 240")
  uu.stderr_is_bytes(r1, b"printf: warning: \x9f\x99\x83: character(s) following character constant have been ignored\n")
  let r2 = uu.invoke(s, "printf", ["ninety seven is %i", "\"a"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "ninety seven is 97")
  let r3 = uu.invoke(s, "printf", ["emoji is %i", "\"🙃"])?
  uu.succeeds(r3)
  uu.stdout_is(r3, "emoji is 240")
  uu.stderr_is_bytes(r3, b"printf: warning: \x9f\x99\x83: character(s) following character constant have been ignored\n")
}

# origin: uutils test_printf::sub_num_int_hex_in
test test_uu_printf_sub_num_int_hex_in { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["twenty is %i", "0x14"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "twenty is 20")
}

# origin: uutils test_printf::sub_num_int_hex_in_neg
test test_uu_printf_sub_num_int_hex_in_neg { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["neg. twenty is %i", "-0x14"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "neg. twenty is -20")
}

# origin: uutils test_printf::sub_num_int_min_width
test test_uu_printf_sub_num_int_min_width { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["twenty is %1i", "20"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "twenty is 20")
}

# origin: uutils test_printf::sub_num_int_neg
test test_uu_printf_sub_num_int_neg { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["neg. twenty is %i", "-20"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "neg. twenty is -20")
}

# origin: uutils test_printf::sub_num_int_oct_in
test test_uu_printf_sub_num_int_oct_in { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["twenty is %i", "024"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "twenty is 20")
}

# origin: uutils test_printf::sub_num_int_oct_in_neg
test test_uu_printf_sub_num_int_oct_in_neg { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["neg. twenty is %i", "-024"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "neg. twenty is -20")
}

# origin: uutils test_printf::sub_num_octal
test test_uu_printf_sub_num_octal { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["twenty in octal is %o", "20"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "twenty in octal is 24")
}

# origin: uutils test_printf::sub_num_sci_lower
test test_uu_printf_sub_num_sci_lower { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["twenty is %e", "20"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "twenty is 2.000000e+01")
}

# origin: uutils test_printf::sub_num_sci_negative
test test_uu_printf_sub_num_sci_negative { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["-1234 is %e", "-1234"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "-1234 is -1.234000e+03")
}

# origin: uutils test_printf::sub_num_sci_trunc
test test_uu_printf_sub_num_sci_trunc { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["pi is ~ %e", "3.1415926535"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "pi is ~ 3.141593e+00")
}

# origin: uutils test_printf::sub_num_sci_upper
test test_uu_printf_sub_num_sci_upper { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["twenty is %E", "20"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "twenty is 2.000000E+01")
}

# origin: uutils test_printf::sub_num_thousands
test test_uu_printf_sub_num_thousands { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%'i", "123456"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "123456")
}

# origin: uutils test_printf::sub_num_uint
test test_uu_printf_sub_num_uint { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["twenty is %u", "20"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "twenty is 20")
}

# origin: uutils test_printf::sub_q_string_empty
test test_uu_printf_sub_q_string_empty { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%q", ""])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "''")
}

# origin: uutils test_printf::sub_q_string_non_printable
test test_uu_printf_sub_q_string_non_printable { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["non-printable: %q", "\"$test\""])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "non-printable: '\"$test\"'")
}

# origin: uutils test_printf::sub_q_string_special_non_printable
test test_uu_printf_sub_q_string_special_non_printable { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["non-printable: %q", "test~"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "non-printable: test~")
}

# origin: uutils test_printf::sub_q_string_validate_field_params
test test_uu_printf_sub_q_string_validate_field_params { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["hello %7q", "world"])?
  uu.fails(r0)
  uu.stdout_is(r0, "hello ")
  uu.stderr_is(r0, "printf: %7q: invalid conversion specification\n")
}

# origin: uutils test_printf::sub_str_max_chars_input
test test_uu_printf_sub_str_max_chars_input { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["hello %7.2s", "world"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "hello      wo")
}

# origin: uutils test_printf::test_emoji_formatting
test test_uu_printf_emoji_formatting { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["Status: %s 🎯 Count: %d\n", "Success 🚀", "42"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "Status: Success 🚀 🎯 Count: 42\n")
}

# origin: uutils test_printf::test_empty_output_succeeds_on_full_device
test test_uu_printf_empty_output_succeeds_on_full_device { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", [""], stdout: p"/dev/full")?
  uu.succeeds(r0)
  uu.no_output(r0)
}

# origin: uutils test_printf::test_extreme_field_width_overflow
test test_uu_printf_extreme_field_width_overflow { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%999999999999999999999999d", "1"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "printf: write error")
}

# origin: uutils test_printf::test_missing_escaped_hex_value
test test_uu_printf_missing_escaped_hex_value { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["\\x"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_only(r0, "printf: missing hexadecimal number in escape\n")
}

# origin: uutils test_printf::test_overflow
test test_uu_printf_overflow { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%d", "36893488147419103232"])?
  uu.fails_with_code(r0, 1)
  uu.stdout_is(r0, "9223372036854775807")
  uu.stderr_is(r0, "printf: '36893488147419103232': Numerical result out of range\n")
  let r1 = uu.invoke(s, "printf", ["%d", "-36893488147419103232"])?
  uu.fails_with_code(r1, 1)
  uu.stdout_is(r1, "-9223372036854775808")
  uu.stderr_is(r1, "printf: '-36893488147419103232': Numerical result out of range\n")
  let r2 = uu.invoke(s, "printf", ["%u", "36893488147419103232"])?
  uu.fails_with_code(r2, 1)
  uu.stdout_is(r2, "18446744073709551615")
  uu.stderr_is(r2, "printf: '36893488147419103232': Numerical result out of range\n")
}

# origin: uutils test_printf::test_q_string_control_chars_with_quotes
test test_uu_printf_q_string_control_chars_with_quotes { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%q", "\x01'\x01"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "''$'\\001'\\'''$'\\001'")
}

# origin: uutils test_printf::test_unterminated_write_error_is_reported
test test_uu_printf_unterminated_write_error_is_reported { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["greeting"], stdout: p"/dev/full")?
  uu.fails(r0)
  uu.stderr_is(r0, "printf: write error: No space left on device\n")
}

# origin: uutils test_printf::test_write_error_omits_errno
test test_uu_printf_write_error_omits_errno { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["\n"], stdout: p"/dev/full")?
  uu.fails_with_code(r0, 1)
  uu.stderr_only(r0, "printf: write error: No space left on device\n")
}

# origin: uutils test_printf::unsigned_hex_negative_wraparound
test test_uu_printf_unsigned_hex_negative_wraparound { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%x", "-0b100"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "fffffffffffffffc")
  let r1 = uu.invoke(s, "printf", ["%x", "-0100"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "ffffffffffffffc0")
  let r2 = uu.invoke(s, "printf", ["%x", "-100"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "ffffffffffffff9c")
  let r3 = uu.invoke(s, "printf", ["%x", "-0x100"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "ffffffffffffff00")
  let r4 = uu.invoke(s, "printf", ["%x", "-92233720368547758150"])?
  uu.fails_with_code(r4, 1)
  uu.stdout_is(r4, "ffffffffffffffff")
  uu.stderr_is(r4, "printf: '-92233720368547758150': Numerical result out of range\n")
  let r5 = uu.invoke(s, "printf", ["%u", "-1002233720368547758150"])?
  uu.fails_with_code(r5, 1)
  uu.stdout_is(r5, "18446744073709551615")
  uu.stderr_is(r5, "printf: '-1002233720368547758150': Numerical result out of range\n")
}

# origin: uutils test_printf::unspecified_left_justify_is_1_width
test test_uu_printf_unspecified_left_justify_is_1_width { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%-o"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "0")
}

# origin: uutils test_printf::value_not_completely_converted_ignores_posixly_correct
test test_uu_printf_value_not_completely_converted_ignores_posixly_correct { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%d", "42abc"], vars: {POSIXLY_CORRECT: "1"})?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "value not completely converted")
}

# origin: uutils test_printf::zero_padding_with_plus_test
test test_uu_printf_zero_padding_with_plus_test { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["%+04d", "1"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "+001")
}

# origin: uutils test_printf::zero_padding_with_space_test
test test_uu_printf_zero_padding_with_space_test { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "printf", ["% 03d", "1"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, " 01")
}

# origin: uutils test_printf::variable_sized_octal
test test_uu_printf_variable_sized_octal { |ctx|
  let s = uu.scene(ctx)?
  for arg in ["|\\5|", "|\\05|", "|\\005|"] {
    let r = uu.invoke(s, "printf", [arg])?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"|\x05|")
  }
  let zero = uu.invoke(s, "printf", ["|\\0005|"])?
  uu.succeeds(zero)
  uu.stdout_only_bytes(zero, b"|\05|")
}

# origin: uutils test_printf::sub_string_char_width_above_u16_max_no_panic
test test_uu_printf_sub_string_char_width_above_u16_max_no_panic { |ctx|
  let s = uu.scene(ctx)?
  let character = uu.invoke(s, "printf", ["%100000c", "A"])?
  uu.succeeds(character)
  uu.stdout_only(character, [" " for _ in range(99999)].join("") + "A")
  let string = uu.invoke(s, "printf", ["%-100000s", "hi"])?
  uu.succeeds(string)
  uu.stdout_only(string, "hi" + [" " for _ in range(99998)].join(""))
}

# origin: uutils test_printf::test_numeric_field_width_above_u16_max
test test_uu_printf_numeric_field_width_above_u16_max { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%65536d", "5"])?
  uu.succeeds(r)
  assert r.stdout.len() == 65536
  for i in range(65535) { assert r.stdout.byte_at(i) == 32 }
  assert r.stdout.byte_at(65535) == 53
}

# origin: uutils test_printf::test_precision_above_formatter_limit
test test_uu_printf_precision_above_formatter_limit { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["%.70123f", "3.25"])?
  uu.succeeds(r)
  assert r.stdout.len() == 70125
  assert r.stdout.starts_with(b"3.25")
  for i in range(4, r.stdout.len()) { assert r.stdout.byte_at(i) == 48 }
}

# origin: uutils test_printf::test_extreme_exponent_does_not_overflow
test test_uu_printf_extreme_exponent_does_not_overflow { |ctx|
  let s = uu.scene(ctx)?
  for pair in [{spec: "%a", zero: "0x0p+0"}, {spec: "%e", zero: "0.000000e+00"}, {spec: "%g", zero: "0"}, {spec: "%f", zero: "0.000000"}] {
    let huge = uu.invoke(s, "printf", [pair.spec, "5e8123456789012345678"])?
    uu.fails_with_code(huge, 1)
    uu.stderr_contains(huge, "Numerical result out of range")
    uu.stdout_contains(huge, "inf")
    let tiny = uu.invoke(s, "printf", [pair.spec, "7E-8123456789012345678"])?
    uu.fails_with_code(tiny, 1)
    uu.stderr_contains(tiny, "Numerical result out of range")
    uu.stdout_is(tiny, pair.zero)
  }
}

# origin: uutils test_printf::test_asterisk_width_i64_min_no_panic
test test_uu_printf_asterisk_width_i64_min_no_panic { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printf", ["|%*d|", "-9223372036854775808", "1"])?
  assert r.status == 0 or r.status == 1
}

