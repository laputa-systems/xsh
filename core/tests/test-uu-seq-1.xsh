##! Transcribed from the uutils seq integration tests.

use support.uu as uu

proc usage_error(r: uu.Ran, message: Str) {
  uu.stderr_only(r, f"seq: {message}\nTry 'seq --help' for more information.\n")
}

# origin: uutils test_seq::test_accepts_option_argument_directly
test test_uu_seq_accepts_option_argument_directly { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-s,"].extend(["2"]), timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1,2\n")
}

# origin: uutils test_seq::test_auto_precision
test test_uu_seq_auto_precision { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["1", "0x1p-1", "2"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1\n1.5\n2\n")
}

# origin: uutils test_seq::test_big_numbers
test test_uu_seq_big_numbers { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", [
  "1000000000000000000000000000",
  "1000000000000000000000000001",
  ], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1000000000000000000000000000\n1000000000000000000000000001\n")
}

# origin: uutils test_seq::test_count_down
test test_uu_seq_count_down { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["--", "5", "-1", "1"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_is(r1, "5\n4\n3\n2\n1\n")
  let r2 = uu.invoke(s, "seq", ["5", "-1", "1"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_is(r2, "5\n4\n3\n2\n1\n")
}

# origin: uutils test_seq::test_count_down_floats
test test_uu_seq_count_down_floats { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["--", "5", "-1.0", "1"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_is(r1, "5.0\n4.0\n3.0\n2.0\n1.0\n")
  let r2 = uu.invoke(s, "seq", ["5", "-1", "1.0"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_is(r2, "5\n4\n3\n2\n1\n")
}

# origin: uutils test_seq::test_count_up
test test_uu_seq_count_up { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["10"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n")
}

# origin: uutils test_seq::test_count_up_floats
test test_uu_seq_count_up_floats { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["10.0"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n")
}

# origin: uutils test_seq::test_default_g_precision
test test_uu_seq_default_g_precision { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-f", "%010g", "1e5", "1e5"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0000100000\n")
  let r2 = uu.invoke(s, "seq", ["-f", "%010g", "1e6", "1e6"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "000001e+06\n")
}

# origin: uutils test_seq::test_drop_negative_zero_end
test test_uu_seq_drop_negative_zero_end { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["1", "-1", "-0"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1\n0\n")
}

# origin: uutils test_seq::test_equal_width_huge_exponent_does_not_overflow
test test_uu_seq_equal_width_huge_exponent_does_not_overflow { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "1e9223372036854775807", "1e-9223372036854775807", "1"], timeout: 3s)?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  usage_error(r1, "invalid floating point argument: '1e9223372036854775807'")
}

# origin: uutils test_seq::test_equalize_widths
test test_uu_seq_equalize_widths { |ctx|
  let s = uu.scene(ctx)?
  let args = ["-w", "--equal-width"]
  for arg in args {
    let r1 = uu.invoke(s, "seq", [arg, "5", "10"], timeout: 3s)?
    uu.succeeds(r1)
    uu.stdout_is(r1, "05\n06\n07\n08\n09\n10\n")
  }
}

# origin: uutils test_seq::test_equalize_widths_corner_cases
test test_uu_seq_equalize_widths_corner_cases { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "0x1", "5.2", "9"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1.0\n6.2\n")
  let r2 = uu.invoke(s, "seq", ["-w", "0x1", "5.2", "10.0000"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_is(r2, "01.0\n06.2\n")
  let r3 = uu.invoke(s, "seq", ["-w", "0x1", "5.2", "15.0000"], timeout: 3s)?
  uu.succeeds(r3)
  uu.stdout_is(r3, "01.0\n06.2\n11.4\n")
  let r4 = uu.invoke(s, "seq", ["-w", "0x1.0000", "5.2", "10"], timeout: 3s)?
  uu.succeeds(r4)
  uu.stdout_is(r4, "1\n6.2\n")
  let r5 = uu.invoke(s, "seq", ["-w", "0x1.1", "1.00002", "3"], timeout: 3s)?
  uu.succeeds(r5)
  uu.stdout_is(r5, "1.0625\n2.06252\n")
}

# origin: uutils test_seq::test_equalize_widths_floats
test test_uu_seq_equalize_widths_floats { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "5", "10.0"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_is(r1, "05\n06\n07\n08\n09\n10\n")
}

# origin: uutils test_seq::test_float_precision_increment
test test_uu_seq_float_precision_increment { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["999", "0.1", "1000.1"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "999.0\n999.1\n999.2\n999.3\n999.4\n999.5\n999.6\n999.7\n999.8\n999.9\n1000.0\n1000.1\n")
}

# origin: uutils test_seq::test_format_and_equal_width
test test_uu_seq_format_and_equal_width { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "-f", "%f", "1"], timeout: 3s)?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "format string may not be specified")
}

# origin: uutils test_seq::test_format_extreme_exponent_does_not_overflow
test test_uu_seq_format_extreme_exponent_does_not_overflow { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", [
  "--format=%a",
  "5e8123456789012345678",
  "5e8123456789012345678",
  ], timeout: 3s)?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  usage_error(r1, "invalid floating point argument: '5e8123456789012345678'")
}

# origin: uutils test_seq::test_format_option
test test_uu_seq_format_option { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-f", "%.2f", "0.0", "0.1", "0.5"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.00\n0.10\n0.20\n0.30\n0.40\n0.50\n")
}

# origin: uutils test_seq::test_format_option_default_precision
test test_uu_seq_format_option_default_precision { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-f", "%f", "0", "0.7", "2"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.000000\n0.700000\n1.400000\n")
}

# origin: uutils test_seq::test_format_option_default_precision_scientific
test test_uu_seq_format_option_default_precision_scientific { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-f", "%E", "0", "0.7", "2"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.000000E+00\n7.000000E-01\n1.400000E+00\n")
}

# origin: uutils test_seq::test_format_option_default_precision_short
test test_uu_seq_format_option_default_precision_short { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-f", "%g", "0", "0.987654321", "2"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0\n0.987654\n1.97531\n")
}

# origin: uutils test_seq::test_format_precision_too_large
test test_uu_seq_format_precision_too_large { |ctx|
  let s = uu.scene(ctx)?
  for spec in ["%.18446744073709551615e", "%.9999999999999999999a"] {
    let r1 = uu.invoke(s, "seq", [f"--format={spec}", "1"], timeout: 3s)?
    uu.fails_with_code(r1, 1)
    uu.no_stdout(r1)
    uu.stderr_contains(r1, "write error: Value too large for defined data type")
  }
}

# origin: uutils test_seq::test_hex_big_number
test test_uu_seq_hex_big_number { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", [
  "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
  "0x100000000000000000000000000000000",
  ], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_is(r1, "340282366920938463463374607431768211456\n")
}

# origin: uutils test_seq::test_hex_identifier_in_wrong_place
test test_uu_seq_hex_identifier_in_wrong_place { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["1234ABCD0x"], timeout: 3s)?
  uu.fails(r1)
  uu.no_stdout(r1)
  usage_error(r1, "invalid floating point argument: '1234ABCD0x'")
}

# origin: uutils test_seq::test_hex_lowercase_uppercase
test test_uu_seq_hex_lowercase_uppercase { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["0xa", "0xA"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_is(r1, "10\n")
  let r2 = uu.invoke(s, "seq", ["0Xa", "0XA"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_is(r2, "10\n")
}

# origin: uutils test_seq::test_hex_rejects_sign_after_identifier
test test_uu_seq_hex_rejects_sign_after_identifier { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["0x-123ABC"], timeout: 3s)?
  uu.fails(r1)
  uu.no_stdout(r1)
  usage_error(r1, "invalid floating point argument: '0x-123ABC'")
  let r2 = uu.invoke(s, "seq", ["0x+123ABC"], timeout: 3s)?
  uu.fails(r2)
  uu.no_stdout(r2)
  usage_error(r2, "invalid floating point argument: '0x+123ABC'")
  let r3 = uu.invoke(s, "seq", ["--", "-0x-123ABC"], timeout: 3s)?
  uu.fails(r3)
  uu.no_stdout(r3)
  usage_error(r3, "invalid floating point argument: '-0x-123ABC'")
  let r4 = uu.invoke(s, "seq", ["--", "-0x+123ABC"], timeout: 3s)?
  uu.fails(r4)
  uu.no_stdout(r4)
  usage_error(r4, "invalid floating point argument: '-0x+123ABC'")
  let r5 = uu.invoke(s, "seq", ["-0x-123ABC"], timeout: 3s)?
  uu.fails(r5)
  uu.no_stdout(r5)
  usage_error(r5, "invalid floating point argument: '-0x-123ABC'")
  let r6 = uu.invoke(s, "seq", ["-0x+123ABC"], timeout: 3s)?
  uu.fails(r6)
  uu.no_stdout(r6)
  usage_error(r6, "invalid floating point argument: '-0x+123ABC'")
}

# origin: uutils test_seq::test_ignore_leading_whitespace
test test_uu_seq_ignore_leading_whitespace { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["   1"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1\n")
}

# origin: uutils test_seq::test_invalid_arg
test test_uu_seq_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["--definitely-invalid"], timeout: 3s)?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_seq::test_invalid_float
test test_uu_seq_invalid_float { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["1e2.3"], timeout: 3s)?
  uu.fails(r1)
  uu.no_stdout(r1)
  usage_error(r1, "invalid floating point argument: '1e2.3'")
  let r2 = uu.invoke(s, "seq", ["1e2.3", "2"], timeout: 3s)?
  uu.fails(r2)
  uu.no_stdout(r2)
  usage_error(r2, "invalid floating point argument: '1e2.3'")
  let r3 = uu.invoke(s, "seq", ["1", "1e2.3"], timeout: 3s)?
  uu.fails(r3)
  uu.no_stdout(r3)
  usage_error(r3, "invalid floating point argument: '1e2.3'")
  let r4 = uu.invoke(s, "seq", ["1e2.3", "2", "3"], timeout: 3s)?
  uu.fails(r4)
  uu.no_stdout(r4)
  usage_error(r4, "invalid floating point argument: '1e2.3'")
  let r5 = uu.invoke(s, "seq", ["1", "1e2.3", "3"], timeout: 3s)?
  uu.fails(r5)
  uu.no_stdout(r5)
  usage_error(r5, "invalid floating point argument: '1e2.3'")
  let r6 = uu.invoke(s, "seq", ["1", "2", "1e2.3"], timeout: 3s)?
  uu.fails(r6)
  uu.no_stdout(r6)
  usage_error(r6, "invalid floating point argument: '1e2.3'")
}

# origin: uutils test_seq::test_invalid_float_point_fail_properly
test test_uu_seq_invalid_float_point_fail_properly { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["66000e0000000000000000000000000000000000000000000000000000092233720368547758070"], timeout: 3s)?
  uu.fails(r1)
  uu.no_stdout(r1)
  usage_error(r1, "invalid floating point argument: '66000e0000000000000000000000000000000000000000000000000000092233720368547758070'")
  let r2 = uu.invoke(s, "seq", ["-1.1e92233720368547758070"], timeout: 3s)?
  uu.fails(r2)
  uu.no_stdout(r2)
  usage_error(r2, "invalid floating point argument: '-1.1e92233720368547758070'")
  let r3 = uu.invoke(s, "seq", ["-.1e92233720368547758070"], timeout: 3s)?
  uu.fails(r3)
  uu.no_stdout(r3)
  usage_error(r3, "invalid floating point argument: '-.1e92233720368547758070'")
}

# origin: uutils test_seq::test_invalid_format
test test_uu_seq_invalid_format { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-f", "%%g", "1"], timeout: 3s)?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "format '%%g' has no % directive")
  let r2 = uu.invoke(s, "seq", ["-f", "%g%g", "1"], timeout: 3s)?
  uu.fails(r2)
  uu.no_stdout(r2)
  uu.stderr_contains(r2, "format '%g%g' has too many % directives")
  let r3 = uu.invoke(s, "seq", ["-f", "%g%", "1"], timeout: 3s)?
  uu.fails(r3)
  uu.no_stdout(r3)
  uu.stderr_contains(r3, "format '%g%' has too many % directives")
  let r4 = uu.invoke(s, "seq", ["-f", "%", "1"], timeout: 3s)?
  uu.fails(r4)
  uu.no_stdout(r4)
  uu.stderr_contains(r4, "format '%' ends in %")
}

# origin: uutils test_seq::test_invalid_zero_increment_value
test test_uu_seq_invalid_zero_increment_value { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["0", "0", "1"], timeout: 3s)?
  uu.fails(r1)
  uu.no_stdout(r1)
  usage_error(r1, "invalid Zero increment value: '0'")
}

# origin: uutils test_seq::test_negative_increment_decimal
test test_uu_seq_negative_increment_decimal { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["0.1", "-0.1", "-0.2"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.1\n0.0\n-0.1\n-0.2\n")
}

# origin: uutils test_seq::test_negative_number_as_separator
test test_uu_seq_negative_number_as_separator { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-s"].extend(["-1", "2"]), timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1-12\n")
}

# origin: uutils test_seq::test_negative_zero_int_start_float_increment
test test_uu_seq_negative_zero_int_start_float_increment { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-0", "0.1", "0.1"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-0.0\n0.1\n")
}

# origin: uutils test_seq::test_no_args
test test_uu_seq_no_args { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", [], timeout: 3s)?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "missing operand")
}

# origin: uutils test_seq::test_option_with_detected_negative_argument
test test_uu_seq_option_with_detected_negative_argument { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-s,"].extend(["-1", "2"]), timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_is(r1, "-1,0,1,2\n")
}

# origin: uutils test_seq::test_parse_error_float
test test_uu_seq_parse_error_float { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["lmnop"], timeout: 3s)?
  uu.fails(r1)
  usage_error(r1, "invalid floating point argument: 'lmnop'")
}

# origin: uutils test_seq::test_parse_error_hex
test test_uu_seq_parse_error_hex { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["0xlmnop"], timeout: 3s)?
  uu.fails(r1)
  usage_error(r1, "invalid floating point argument: '0xlmnop'")
}

# origin: uutils test_seq::test_parse_out_of_bounds_exponents
test test_uu_seq_parse_out_of_bounds_exponents { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["1e-9223372036854775808"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "")
  let r2 = uu.invoke(s, "seq", ["1e-922337203685477580800000000", "1"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "0\n1\n")
  let r3 = uu.invoke(s, "seq", ["-1e-922337203685477580800000000", "1"], timeout: 3s)?
  uu.succeeds(r3)
  uu.stdout_only(r3, "-0\n1\n")
}

# origin: uutils test_seq::test_parse_scientific_zero
test test_uu_seq_parse_scientific_zero { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["0e15", "1"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0\n1\n")
  let r2 = uu.invoke(s, "seq", ["0.0e15", "1"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "0\n1\n")
  let r3 = uu.invoke(s, "seq", ["0", "1"], timeout: 3s)?
  uu.succeeds(r3)
  uu.stdout_only(r3, "0\n1\n")
  let r4 = uu.invoke(s, "seq", ["-w", "0e15", "1"], timeout: 3s)?
  uu.succeeds(r4)
  uu.stdout_only(r4, "0000000000000000\n0000000000000001\n")
  let r5 = uu.invoke(s, "seq", ["-w", "0.0e15", "1"], timeout: 3s)?
  uu.succeeds(r5)
  uu.stdout_only(r5, "0000000000000000\n0000000000000001\n")
  let r6 = uu.invoke(s, "seq", ["-w", "0", "1"], timeout: 3s)?
  uu.succeeds(r6)
  uu.stdout_only(r6, "0\n1\n")
}

# origin: uutils test_seq::test_power_of_ten_display
test test_uu_seq_power_of_ten_display { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-f", "%.2g", "10", "10"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "10\n")
}

# origin: uutils test_seq::test_precision_corner_cases
test test_uu_seq_precision_corner_cases { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["0x1", "0.90", "3"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1.00\n1.90\n2.80\n")
  let r2 = uu.invoke(s, "seq", ["0x1.00", "0.90", "3"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_is(r2, "1\n1.9\n2.8\n")
  let r3 = uu.invoke(s, "seq", ["1", "1.20", "0x3.000000"], timeout: 3s)?
  uu.succeeds(r3)
  uu.stdout_is(r3, "1\n2.2\n")
  let r4 = uu.invoke(s, "seq", ["1", "1.20", "3.000000"], timeout: 3s)?
  uu.succeeds(r4)
  uu.stdout_is(r4, "1.00\n2.20\n")
}

# origin: uutils test_seq::test_preserve_negative_zero_start
test test_uu_seq_preserve_negative_zero_start { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-0", "1"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-0\n1\n")
  let r2 = uu.invoke(s, "seq", ["-0", "1", "2"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "-0\n1\n2\n")
  let r3 = uu.invoke(s, "seq", ["-0", "1", "2.0"], timeout: 3s)?
  uu.succeeds(r3)
  uu.stdout_only(r3, "-0\n1\n2\n")
}

# origin: uutils test_seq::test_rejects_nan
test test_uu_seq_rejects_nan { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["NaN"], timeout: 3s)?
  uu.fails(r1)
  usage_error(r1, "invalid 'not-a-number' argument: 'NaN'")
}

# origin: uutils test_seq::test_rejects_non_floats
test test_uu_seq_rejects_non_floats { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["foo"], timeout: 3s)?
  uu.fails(r1)
  usage_error(r1, "invalid floating point argument: 'foo'")
}

# origin: uutils test_seq::test_rounding_end
test test_uu_seq_rounding_end { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["1", "-1", "0.1"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1\n")
}

# origin: uutils test_seq::test_separator_and_terminator
test test_uu_seq_separator_and_terminator { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-s", ",", "-t", "!", "2", "6"], timeout: 3s)?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "seq: invalid option -- 't'\nTry 'seq --help' for more information.\n")
  let r2 = uu.invoke(s, "seq", ["-s", ",", "2", "6"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_is(r2, "2,3,4,5,6\n")
  let r3 = uu.invoke(s, "seq", ["-s", "", "2", "6"], timeout: 3s)?
  uu.succeeds(r3)
  uu.stdout_is(r3, "23456\n")
  let r4 = uu.invoke(s, "seq", ["-s", "\n", "2", "6"], timeout: 3s)?
  uu.succeeds(r4)
  uu.stdout_is(r4, "2\n3\n4\n5\n6\n")
  let r5 = uu.invoke(s, "seq", ["-s", "\\n", "2", "6"], timeout: 3s)?
  uu.succeeds(r5)
  uu.stdout_is(r5, "2\\n3\\n4\\n5\\n6\n")
}

# origin: uutils test_seq::test_seq_float_precision_edge_cases
test test_uu_seq_seq_float_precision_edge_cases { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", [".64999", "1e-7", ".6499901"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.6499900\n0.6499901\n")
  let r2 = uu.invoke(s, "seq", ["0", "0.000002", "0.000006"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "0.000000\n0.000002\n0.000004\n0.000006\n")
}

# origin: uutils test_seq::test_seq_wrong_arg
test test_uu_seq_seq_wrong_arg { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "5", "10", "33", "32"], timeout: 3s)?
  uu.fails(r1)
}

# origin: uutils test_seq::test_seq_wrong_arg_floats
test test_uu_seq_seq_wrong_arg_floats { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "5", "10.0", "33", "32"], timeout: 3s)?
  uu.fails(r1)
}

# origin: uutils test_seq::test_trailing_whitespace_error
test test_uu_seq_trailing_whitespace_error { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["1 "], timeout: 3s)?
  uu.fails(r1)
  usage_error(r1, "invalid floating point argument: '1 '")
}

# origin: uutils test_seq::test_undefined
test test_uu_seq_undefined { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["1e-9223372036854775808"], timeout: 3s)?
  uu.succeeds(r1)
  uu.no_output(r1)
}

# origin: uutils test_seq::test_width_decimal_scientific_notation_increment
test test_uu_seq_width_decimal_scientific_notation_increment { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", ".1", "1e-2", ".11"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.10\n0.11\n")
  let r2 = uu.invoke(s, "seq", ["-w", ".0", "1.500e-1", ".2"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "0.0000\n0.1500\n")
}

# origin: uutils test_seq::test_width_decimal_scientific_notation_trailing_zeros_end
test test_uu_seq_width_decimal_scientific_notation_trailing_zeros_end { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "1e-1", "1e-2", ".1100"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.10\n0.11\n")
}

# origin: uutils test_seq::test_width_decimal_scientific_notation_trailing_zeros_increment
test test_uu_seq_width_decimal_scientific_notation_trailing_zeros_increment { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "1e-1", "0.0100", ".11"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.1000\n0.1100\n")
}

# origin: uutils test_seq::test_width_decimal_scientific_notation_trailing_zeros_start
test test_uu_seq_width_decimal_scientific_notation_trailing_zeros_start { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", ".1000", "1e-2", ".11"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0.1000\n0.1100\n")
}

# origin: uutils test_seq::test_width_floats
test test_uu_seq_width_floats { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "9.0", "10.0"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "09.0\n10.0\n")
}

# origin: uutils test_seq::test_width_invalid_float
test test_uu_seq_width_invalid_float { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "1e2.3"], timeout: 3s)?
  uu.fails(r1)
  uu.no_stdout(r1)
  usage_error(r1, "invalid floating point argument: '1e2.3'")
}

# origin: uutils test_seq::test_width_negative_decimal_notation
test test_uu_seq_width_negative_decimal_notation { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "-.1", ".1", ".11"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-0.1\n00.0\n00.1\n")
}

# origin: uutils test_seq::test_width_negative_scientific_notation
test test_uu_seq_width_negative_scientific_notation { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "-1e-3", "1"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-0.001\n00.999\n")
  let r2 = uu.invoke(s, "seq", ["-w", "-1.e-3", "1"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "-0.001\n00.999\n")
  let r3 = uu.invoke(s, "seq", ["-w", "-1.0e-4", "1"], timeout: 3s)?
  uu.succeeds(r3)
  uu.stdout_only(r3, "-0.00010\n00.99990\n")
  let r4 = uu.invoke(s, "seq", ["-w", "-.1e2", "10", "100"], timeout: 3s)?
  uu.succeeds(r4)
  uu.stdout_only(r4, "-010\n0000\n0010\n0020\n0030\n0040\n0050\n0060\n0070\n0080\n0090\n0100\n")
  let r5 = uu.invoke(s, "seq", ["-w", "-0.1e2", "10", "100"], timeout: 3s)?
  uu.succeeds(r5)
  uu.stdout_only(r5, "-010\n0000\n0010\n0020\n0030\n0040\n0050\n0060\n0070\n0080\n0090\n0100\n")
}

# origin: uutils test_seq::test_width_negative_zero
test test_uu_seq_width_negative_zero { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "-0", "1"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-0\n01\n")
  let r2 = uu.invoke(s, "seq", ["-w", "-0", "1", "2"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "-0\n01\n02\n")
  let r3 = uu.invoke(s, "seq", ["-w", "-0", "1", "2.0"], timeout: 3s)?
  uu.succeeds(r3)
  uu.stdout_only(r3, "-0\n01\n02\n")
}

# origin: uutils test_seq::test_width_negative_zero_decimal_notation
test test_uu_seq_width_negative_zero_decimal_notation { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "-0.0", "1"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-0.0\n01.0\n")
  let r2 = uu.invoke(s, "seq", ["-w", "-0.0", "1.0"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "-0.0\n01.0\n")
  let r3 = uu.invoke(s, "seq", ["-w", "-0.0", "1", "2"], timeout: 3s)?
  uu.succeeds(r3)
  uu.stdout_only(r3, "-0.0\n01.0\n02.0\n")
  let r4 = uu.invoke(s, "seq", ["-w", "-0.0", "1", "2.0"], timeout: 3s)?
  uu.succeeds(r4)
  uu.stdout_only(r4, "-0.0\n01.0\n02.0\n")
  let r5 = uu.invoke(s, "seq", ["-w", "-0.0", "1.0", "2"], timeout: 3s)?
  uu.succeeds(r5)
  uu.stdout_only(r5, "-0.0\n01.0\n02.0\n")
  let r6 = uu.invoke(s, "seq", ["-w", "-0.0", "1.0", "2.0"], timeout: 3s)?
  uu.succeeds(r6)
  uu.stdout_only(r6, "-0.0\n01.0\n02.0\n")
}

# origin: uutils test_seq::test_width_negative_zero_scientific_notation
test test_uu_seq_width_negative_zero_scientific_notation { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "-0e0", "1"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-0\n01\n")
  let r2 = uu.invoke(s, "seq", ["-w", "-0e0", "1", "2"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "-0\n01\n02\n")
  let r3 = uu.invoke(s, "seq", ["-w", "-0e0", "1", "2.0"], timeout: 3s)?
  uu.succeeds(r3)
  uu.stdout_only(r3, "-0\n01\n02\n")
  let r4 = uu.invoke(s, "seq", ["-w", "-0e+1", "1"], timeout: 3s)?
  uu.succeeds(r4)
  uu.stdout_only(r4, "-00\n001\n")
  let r5 = uu.invoke(s, "seq", ["-w", "-0e+1", "1", "2"], timeout: 3s)?
  uu.succeeds(r5)
  uu.stdout_only(r5, "-00\n001\n002\n")
  let r6 = uu.invoke(s, "seq", ["-w", "-0e+1", "1", "2.0"], timeout: 3s)?
  uu.succeeds(r6)
  uu.stdout_only(r6, "-00\n001\n002\n")
  let r7 = uu.invoke(s, "seq", ["-w", "-0.000e0", "1"], timeout: 3s)?
  uu.succeeds(r7)
  uu.stdout_only(r7, "-0.000\n01.000\n")
  let r8 = uu.invoke(s, "seq", ["-w", "-0.000e0", "1", "2"], timeout: 3s)?
  uu.succeeds(r8)
  uu.stdout_only(r8, "-0.000\n01.000\n02.000\n")
  let r9 = uu.invoke(s, "seq", ["-w", "-0.000e0", "1", "2.0"], timeout: 3s)?
  uu.succeeds(r9)
  uu.stdout_only(r9, "-0.000\n01.000\n02.000\n")
  let r10 = uu.invoke(s, "seq", ["-w", "-0.000e-2", "1"], timeout: 3s)?
  uu.succeeds(r10)
  uu.stdout_only(r10, "-0.00000\n01.00000\n")
  let r11 = uu.invoke(s, "seq", ["-w", "-0.000e-2", "1", "2"], timeout: 3s)?
  uu.succeeds(r11)
  uu.stdout_only(r11, "-0.00000\n01.00000\n02.00000\n")
  let r12 = uu.invoke(s, "seq", ["-w", "-0.000e-2", "1", "2.0"], timeout: 3s)?
  uu.succeeds(r12)
  uu.stdout_only(r12, "-0.00000\n01.00000\n02.00000\n")
  let r13 = uu.invoke(s, "seq", ["-w", "-0.000e5", "1"], timeout: 3s)?
  uu.succeeds(r13)
  uu.stdout_only(r13, "-000000\n0000001\n")
  let r14 = uu.invoke(s, "seq", ["-w", "-0.000e5", "1", "2"], timeout: 3s)?
  uu.succeeds(r14)
  uu.stdout_only(r14, "-000000\n0000001\n0000002\n")
  let r15 = uu.invoke(s, "seq", ["-w", "-0.000e5", "1", "2.0"], timeout: 3s)?
  uu.succeeds(r15)
  uu.stdout_only(r15, "-000000\n0000001\n0000002\n")
  let r16 = uu.invoke(s, "seq", ["-w", "-0.000e5", "1"], timeout: 3s)?
  uu.succeeds(r16)
  uu.stdout_only(r16, "-000000\n0000001\n")
  let r17 = uu.invoke(s, "seq", ["-w", "-0.000e5", "1", "2"], timeout: 3s)?
  uu.succeeds(r17)
  uu.stdout_only(r17, "-000000\n0000001\n0000002\n")
  let r18 = uu.invoke(s, "seq", ["-w", "-0.000e5", "1", "2.0"], timeout: 3s)?
  uu.succeeds(r18)
  uu.stdout_only(r18, "-000000\n0000001\n0000002\n")
}

# origin: uutils test_seq::test_width_scientific_notation
test test_uu_seq_width_scientific_notation { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "999", "1e3"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0999\n1000\n")
  let r2 = uu.invoke(s, "seq", ["-w", "999", "1E3"], timeout: 3s)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "0999\n1000\n")
}

# origin: uutils test_seq::test_zero_not_first
test test_uu_seq_zero_not_first { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "seq", ["-w", "-0.1", "0.1", "0.1"], timeout: 3s)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "-0.1\n00.0\n00.1\n")
}

# Prefix reads are bounded; the live reader applies pipe backpressure to the producer.
proc read_prefix(fd: Int, length: Int) [process, error] -> Result[Bytes, Error] {
  var result = b""
  while result.len() < length {
    assert "readable" in unix.poll_fd(fd, ["readable"], timeout_ms: 5000)?
    let chunk = unix.read_fd(fd, length - result.len())?
    assert !chunk.is_empty(), "sequence ended before its expected prefix"
    result = bytes.concat([result, chunk])
  }
  Ok(result)
}

# Closing the sole pipe reader must let the producer terminate with SIGPIPE.
proc infinite_prefix(s: uu.Scene, args: List[Str], expected: Bytes) [fs, process, env, error] {
  uu.mkfifo(s, "output-pipe")?
  let reader = unix.open_fd(uu.at(s, "output-pipe"), nonblock: true)?
  var reader_open = true
  defer { if reader_open { unix.close_fd(reader)? } }
  let command = uu.command(s, "seq", args, stdout: uu.at(s, "output-pipe"), stderr: uu.at(s, "stderr"), timeout: 5s)?
  let child = spawn command?
  defer child.cancel(signal: "KILL", kill_after: 0ms)?
  let prefix = read_prefix(reader, expected.len())?
  unix.close_fd(reader)?
  reader_open = false
  assert prefix == expected
  let status = wait child?
  assert status.signaled()
  assert status.signal_number()? == process.signal("PIPE")?.number
}

# origin: uutils test_seq::test_separator_non_utf8
test test_uu_seq_separator_non_utf8 { |ctx|
  let s = uu.scene(ctx)?
  for arg in [b"-s\xff\xfe", b"--separator=\xff\xfe"] {
    let r = uu.invoke_paths(s, "seq", [Path.parse_bytes(arg)?, p"2"], timeout: 3s)?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, b"1\xff\xfe2\n")
  }
}

# origin: uutils test_seq::test_parse_valid_hexadecimal_float_two_args
test test_uu_seq_parse_valid_hexadecimal_float_two_args { |ctx|
  let s = uu.scene(ctx)?
  for case in [
    {args: ["0x1p-1", "2"], expected: "0.5\n1.5\n"},
    {args: ["0x.8p16", "32768"], expected: "32768\n"},
    {args: ["0xffff.4p-4", "4096"], expected: "4095.95\n"},
    {args: ["0xA.A9p-1", "6"], expected: "5.33008\n"},
    {args: ["0xa.a9p-1", "6"], expected: "5.33008\n"},
    {args: ["0xffffffffffp-30", "1024"], expected: "1024\n"},
    {args: ["  0XA.A9P-1", "6"], expected: "5.33008\n"},
    {args: ["  0xee.", "  0xef."], expected: "238\n239\n"},
  ] {
    let r = uu.invoke(s, "seq", case.args, timeout: 3s)?
    uu.succeeds(r)
    uu.stdout_only(r, case.expected)
  }
}

# origin: uutils test_seq::test_parse_valid_hexadecimal_float_three_args
test test_uu_seq_parse_valid_hexadecimal_float_three_args { |ctx|
  let s = uu.scene(ctx)?
  for case in [
    {args: ["0x3.4p-1", "0x4p-1", "4"], expected: "1.625\n3.625\n"},
    {args: ["-0x.ep-3", "-0x.1p-3", "-0x.fp-3"], expected: "-0.109375\n-0.117188\n"},
  ] {
    let r = uu.invoke(s, "seq", case.args, timeout: 3s)?
    uu.succeeds(r)
    uu.stdout_only(r, case.expected)
  }
}

# origin: uutils test_seq::test_format_precision_above_formatter_limit
test test_uu_seq_format_precision_above_formatter_limit { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["-f", "%.66000f", "4", "4"], timeout: 3s)?
  uu.succeeds(r)
  let output = r.stdout.utf8()?
  assert output.byte_len() == 66003
  assert output.starts_with("4.")
  let digits = [char for char in output.byte_slice(2)]
  var end = digits.len()
  while end > 0 and digits[end - 1].trim() == "" { end -= 1 }
  for char in digits[..end] { assert char == "0" }
}

# origin: uutils test_seq::test_sigpipe_ignored_reports_write_error
test test_uu_seq_sigpipe_ignored_reports_write_error { |ctx|
  let s = uu.scene(ctx)?
  let words = uu.argv(s, "seq", [p"inf"])?
  let argv = [p"/bin/sh", p"-c",
    p"trap '' PIPE; { \"$@\" 2>err; echo $? >code; } | head -n1", p"seq-pipe"].extend(words)
  let output = uu.at(s, "stdout")
  let errors = uu.at(s, "stderr")
  let command = process.command_argv(p"/bin/sh", argv, s.root, {}, b"", output, errors, timeout: 5s)
  assert process.run(command)?.exited_with(0)
  assert output.read_bytes()? == b"1\n"
  assert "seq: write error: Broken pipe" in uu.read_text(s, "err")?
  uu.file_is(s, "code", "1\n")?
}

# origin: uutils test_seq::test_infinite_sequence::case_1_neg_inf
test test_uu_seq_test_infinite_sequence_case_1_neg_inf { |ctx|
  let s = uu.scene(ctx)?
  infinite_prefix(s, ["--", "-inf", "0"], b"-inf\n-inf\n-inf\n")?
}

# origin: uutils test_seq::test_infinite_sequence::case_2_neg_infinity
test test_uu_seq_test_infinite_sequence_case_2_neg_infinity { |ctx|
  let s = uu.scene(ctx)?
  infinite_prefix(s, ["--", "-infinity", "0"], b"-inf\n-inf\n-inf\n")?
}

# origin: uutils test_seq::test_infinite_sequence::case_3_inf
test test_uu_seq_test_infinite_sequence_case_3_inf { |ctx|
  let s = uu.scene(ctx)?
  infinite_prefix(s, ["inf"], b"1\n2\n3\n")?
}

# origin: uutils test_seq::test_infinite_sequence::case_4_infinity
test test_uu_seq_test_infinite_sequence_case_4_infinity { |ctx|
  let s = uu.scene(ctx)?
  infinite_prefix(s, ["infinity"], b"1\n2\n3\n")?
}

# origin: uutils test_seq::test_infinite_sequence::case_5_inf_width
test test_uu_seq_test_infinite_sequence_case_5_inf_width { |ctx|
  let s = uu.scene(ctx)?
  infinite_prefix(s, ["-w", "1.000", "inf", "inf"], b"1.000\n  inf\n  inf\n  inf\n")?
}

# origin: uutils test_seq::test_infinite_sequence::case_6_neg_inf_width
test test_uu_seq_test_infinite_sequence_case_6_neg_inf_width { |ctx|
  let s = uu.scene(ctx)?
  infinite_prefix(s, ["-w", "1.000", "-inf", "-inf"], b"1.000\n -inf\n -inf\n -inf\n")?
}

# origin: uutils test_seq::test_infinite_sequence::case_7_precision_inf
test test_uu_seq_test_infinite_sequence_case_7_precision_inf { |ctx|
  let s = uu.scene(ctx)?
  infinite_prefix(s, ["1", "1.2", "inf"], b"1.0\n2.2\n3.4\n")?
}

# origin: uutils test_seq::test_infinite_sequence::case_8_equalize_width_inf
test test_uu_seq_test_infinite_sequence_case_8_equalize_width_inf { |ctx|
  let s = uu.scene(ctx)?
  infinite_prefix(s, ["-w", "1", "1.2", "inf"], b"1.0\n2.2\n3.4\n")?
}
