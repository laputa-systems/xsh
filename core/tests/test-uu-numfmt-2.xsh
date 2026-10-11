##! Transcribed from the uutils coreutils integration tests for numfmt.

use support.uu as uu

# origin: uutils test_numfmt::test_invalid_arg_number_with_warn_returns_status_0
test test_uu_numfmt_invalid_arg_number_with_warn_returns_status_0 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--invalid=warn", "4Q"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "4Q\n")
  uu.stderr_is(r1, "numfmt: rejecting suffix in input: '4Q' (consider using --from)\n")
}

# origin: uutils test_numfmt::test_invalid_argument_returns_status_1
test test_uu_numfmt_invalid_argument_returns_status_1 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--header=hello"], stdin: bytes.from_text("53478"))?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_numfmt::test_invalid_fail_with_fields_does_not_duplicate_output
test test_uu_numfmt_invalid_fail_with_fields_does_not_duplicate_output { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--invalid=fail", "--field=2", "--from=si", "--to=iec"], stdin: bytes.from_text("A 1K x\nB Foo y\nC 3G z\n"))?
  uu.fails_with_code(r1, 2)
  uu.stdout_is(r1, "A 1000 x\nB Foo y\nC 2.8G z\n")
  uu.stderr_is(r1, "numfmt: invalid number: 'Foo'\n")
}

# origin: uutils test_numfmt::test_invalid_following_valid_suffix
test test_uu_numfmt_invalid_following_valid_suffix { |ctx|
  let s = uu.scene(ctx)?
  for valid_suffix in ["K", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q", "k"] {
    for c in ["ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz".byte_slice(i, length: 1) for i in range(52)] {
      let r = uu.invoke(s, "numfmt", ["--from=si", "--to=si", f"1{valid_suffix}{c}"])?
      uu.fails_with_code(r, 2)
      uu.stderr_only(r, f"numfmt: invalid suffix in input '1{valid_suffix}{c}': '{c}'\n")
    }
  }
}

# origin: uutils test_numfmt::test_invalid_padding_value
test test_uu_numfmt_invalid_padding_value { |ctx|
  let s = uu.scene(ctx)?
  for padding_value in ["A", "0"] {
    let r = uu.invoke(s, "numfmt", [f"--padding={padding_value}", "5"])?
    uu.fails_with_code(r, 1)
    uu.stderr_contains(r, f"invalid padding value '{padding_value}'")
  }
}

# origin: uutils test_numfmt::test_invalid_stdin_number_in_middle_of_input
test test_uu_numfmt_invalid_stdin_number_in_middle_of_input { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", [], stdin: bytes.from_text("100\nhello\n200"))?
  uu.fails_with_code(r1, 2)
  uu.stdout_is(r1, "100\n")
}

# origin: uutils test_numfmt::test_invalid_stdin_number_returns_status_2
test test_uu_numfmt_invalid_stdin_number_returns_status_2 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", [], stdin: bytes.from_text("hello"))?
  uu.fails_with_code(r1, 2)
}

# origin: uutils test_numfmt::test_invalid_stdin_number_with_abort_returns_status_2
test test_uu_numfmt_invalid_stdin_number_with_abort_returns_status_2 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--invalid=abort"], stdin: bytes.from_text("4Q"))?
  uu.fails_with_code(r1, 2)
  uu.stderr_only(r1, "numfmt: rejecting suffix in input: '4Q' (consider using --from)\n")
}

# origin: uutils test_numfmt::test_invalid_stdin_number_with_fail_returns_status_2
test test_uu_numfmt_invalid_stdin_number_with_fail_returns_status_2 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--invalid=fail"], stdin: bytes.from_text("4Q"))?
  uu.fails_with_code(r1, 2)
  uu.stdout_is(r1, "4Q")
  uu.stderr_is(r1, "numfmt: rejecting suffix in input: '4Q' (consider using --from)\n")
}

# origin: uutils test_numfmt::test_invalid_stdin_number_with_ignore_returns_status_0
test test_uu_numfmt_invalid_stdin_number_with_ignore_returns_status_0 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--invalid=ignore"], stdin: bytes.from_text("4Q"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "4Q")
}

# origin: uutils test_numfmt::test_invalid_stdin_number_with_warn_returns_status_0
test test_uu_numfmt_invalid_stdin_number_with_warn_returns_status_0 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--invalid=warn"], stdin: bytes.from_text("4Q"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "4Q")
  uu.stderr_is(r1, "numfmt: rejecting suffix in input: '4Q' (consider using --from)\n")
}

# origin: uutils test_numfmt::test_invalid_unit_size
test test_uu_numfmt_invalid_unit_size { |ctx|
  let s = uu.scene(ctx)?
  for command in ["from", "to"] {
    for invalid_size in ["A", "0", "18446744073709551616", "18446744073709551615K", "18014398509481984Ki"] {
      let r = uu.invoke(s, "numfmt", [f"--{command}-unit={invalid_size}"])?
      uu.fails_with_code(r, 1)
      uu.stderr_contains(r, f"invalid unit size: '{invalid_size}'")
    }
  }
}

# origin: uutils test_numfmt::test_invalid_utf8_input
test test_uu_numfmt_invalid_utf8_input { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", [], stdin: b"10\n\xFF")?
  uu.fails_with_code(r1, 2)
  uu.stdout_is(r1, "10\n")
  uu.stderr_is(r1, "numfmt: invalid number: '\\377'\n")
}

# origin: uutils test_numfmt::test_large_integer_precision_loss_issue_11654
test test_uu_numfmt_large_integer_precision_loss_issue_11654 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=iec", "9153396227555392131"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "9153396227555392131\n")
}

# origin: uutils test_numfmt::test_leading_whitespace_in_free_argument_should_imply_padding
test test_uu_numfmt_leading_whitespace_in_free_argument_should_imply_padding { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=auto", "   1Ki"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "  1024\n")
}

# origin: uutils test_numfmt::test_leading_whitespace_should_imply_padding
test test_uu_numfmt_leading_whitespace_should_imply_padding { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=auto"], stdin: bytes.from_text("   1K"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, " 1000")
  let r2 = uu.invoke(s, "numfmt", ["--from=auto"], stdin: bytes.from_text("    202Ki"))?
  uu.succeeds(r2)
  uu.stdout_is(r2, "   206848")
}

# origin: uutils test_numfmt::test_line_is_field_with_no_delimiter
test test_uu_numfmt_line_is_field_with_no_delimiter { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["-d,", "--to=iec"], stdin: bytes.from_text("123456"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "121K")
}

# origin: uutils test_numfmt::test_locale_c_uses_period
test test_uu_numfmt_locale_c_uses_period { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--to=iec", "1500"], vars: {LC_ALL: "C"})?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1.5K\n")
}

# origin: uutils test_numfmt::test_locale_fr_input_comma
test test_uu_numfmt_locale_fr_input_comma { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--format=%.3f", "1,5"], vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1,500\n")
}

# origin: uutils test_numfmt::test_locale_fr_output
test test_uu_numfmt_locale_fr_output { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--to=iec", "1500"], vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1,5K\n")
}

# origin: uutils test_numfmt::test_locale_fr_rejects_period
test test_uu_numfmt_locale_fr_rejects_period { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--format=%.3f", "1.5"], vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.fails(r1)
  uu.stderr_contains(r1, "invalid")
}

# origin: uutils test_numfmt::test_long_invalid_suffix
test test_uu_numfmt_long_invalid_suffix { |ctx|
  let s = uu.scene(ctx)?
  let args = ["--from=si", "--to=si", "1500VVVVVVVV"]
  let r1 = uu.invoke(s, "numfmt", args)?
  uu.fails_with_code(r1, 2)
  uu.stderr_only(r1, "numfmt: invalid suffix in input: '1500VVVVVVVV'\n")
}

# origin: uutils test_numfmt::test_multibyte_suffix_issue11937
test test_uu_numfmt_multibyte_suffix_issue11937 { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--suffix=€", "--format=%10.2f", "692"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "692.00€\n")
}

# origin: uutils test_numfmt::test_negative
test test_uu_numfmt_negative { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=si"], stdin: bytes.from_text("-1000\n-1.1M\n-0.1G"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "-1000\n-1100000\n-100000000")
  let r2 = uu.invoke(s, "numfmt", ["--to=iec-i"], stdin: bytes.from_text("-1024\n-1153434\n-107374182"))?
  uu.succeeds(r2)
  uu.stdout_is(r2, "-1.0Ki\n-1.2Mi\n-103Mi")
}

# origin: uutils test_numfmt::test_negative_padding
test test_uu_numfmt_negative_padding { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=si", "--padding=-8"], stdin: bytes.from_text("1K\n1.1M\n0.1G"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1000    \n1100000 \n100000000")
}

# origin: uutils test_numfmt::test_negative_padding_as_separate_arg
test test_uu_numfmt_negative_padding_as_separate_arg { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=si", "--padding", "-8"], stdin: bytes.from_text("1K\n1.1M\n0.1G"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1000    \n1100000 \n100000000")
}

# origin: uutils test_numfmt::test_negative_zero
test test_uu_numfmt_negative_zero { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", [], stdin: bytes.from_text("-0\n-0.0"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "0\n0.0")
}

# origin: uutils test_numfmt::test_no_op
test test_uu_numfmt_no_op { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", [], stdin: bytes.from_text("1024\n1234567"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1024\n1234567")
}

# origin: uutils test_numfmt::test_non_utf8_delimiter
test test_uu_numfmt_non_utf8_delimiter { |ctx|
  let s = uu.scene(ctx)?
  for delim in [b"\xff", b"\xa2\xe3"] {
    let input = bytes.concat([b"1", delim, b"2K"])
    let expected = bytes.concat([b"1", delim, b"2000\n"])
    let r = uu.invoke_paths(s, "numfmt", [p"--from=si", p"--field=2", p"-d", Path.parse_bytes(delim)?, Path.parse_bytes(input)?])?
    if delim.len() == 1 {
      uu.succeeds(r)
      uu.stdout_is_bytes(r, expected)
    } else {
      uu.fails_with_code(r, 1)
      uu.stderr_only(r, "numfmt: the delimiter must be a single character\n")
    }
  }
}

# origin: uutils test_numfmt::test_normalize
test test_uu_numfmt_normalize { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=si", "--to=si"], stdin: bytes.from_text("10000000K\n0.001K"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "10G\n1")
}

# origin: uutils test_numfmt::test_null_byte_input
test test_uu_numfmt_null_byte_input { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", [], stdin: bytes.from_text("1000\0\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1000\n")
  let r2 = uu.invoke(s, "numfmt", [], stdin: bytes.from_text("1000\0"))?
  uu.succeeds(r2)
  uu.stdout_is(r2, "1000")
}

# origin: uutils test_numfmt::test_null_byte_input_multiline
test test_uu_numfmt_null_byte_input_multiline { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", [], stdin: bytes.from_text("1000\0\n2000\0"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1000\n2000")
  let r2 = uu.invoke(s, "numfmt", [], stdin: bytes.from_text("1000\02000\n3000"))?
  uu.succeeds(r2)
  uu.stdout_is(r2, "1000\n3000")
}

# origin: uutils test_numfmt::test_numfmt_negative_after_double_dash_ok
test test_uu_numfmt_numfmt_negative_after_double_dash_ok { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--to=iec", "--", "-8765432"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "-8.4M\n")
}

# origin: uutils test_numfmt::test_numfmt_scientific_notation_rejected
test test_uu_numfmt_numfmt_scientific_notation_rejected { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["2e8"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_contains(r1, "invalid suffix in input")
}

# origin: uutils test_numfmt::test_padding
test test_uu_numfmt_padding { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=si", "--padding=8"], stdin: bytes.from_text("1K\n1.1M\n0.1G"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "    1000\n 1100000\n100000000")
}

# origin: uutils test_numfmt::test_reject_leading_plus_and_scientific_notation
test test_uu_numfmt_reject_leading_plus_and_scientific_notation { |ctx|
  let s = uu.scene(ctx)?
  for input in ["+5", "+5K", "+1e-3"] {
    let r = uu.invoke(s, "numfmt", ["--from=auto"], stdin: bytes.from_text(f"{input}\n"))?
    uu.fails_with_code(r, 2)
    uu.stderr_is(r, f"numfmt: invalid number: '{input}'\n")
  }
  for input in ["1e-3", "1e+3", "5e-5", "1.0e-2", "-1e-5"] {
    let r = uu.invoke(s, "numfmt", ["--from=auto"], stdin: bytes.from_text(f"{input}\n"))?
    uu.fails_with_code(r, 2)
    uu.stderr_is(r, f"numfmt: invalid suffix in input: '{input}'\n")
  }
}

# origin: uutils test_numfmt::test_rejects_malformed_number_forms
test test_uu_numfmt_rejects_malformed_number_forms { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=si", "12.K"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_contains(r1, "invalid number: '12.K'")
  let r2 = uu.invoke(s, "numfmt", ["--from=si", "--delimiter=,", "12.  2"])?
  uu.fails_with_code(r2, 2)
  uu.stderr_contains(r2, "invalid number: '12.  2'")
  let r3 = uu.invoke(s, "numfmt", ["..1"])?
  uu.fails_with_code(r3, 2)
  uu.stderr_contains(r3, "invalid suffix in input: '..1'")
}

# origin: uutils test_numfmt::test_round
test test_uu_numfmt_round { |ctx|
  let s = uu.scene(ctx)?
  for case in [
    {method: "from-zero", expected: ["9.1k", "-9.1k", "9.1k", "-9.1k"]},
    {method: "from-zer", expected: ["9.1k", "-9.1k", "9.1k", "-9.1k"]},
    {method: "f", expected: ["9.1k", "-9.1k", "9.1k", "-9.1k"]},
    {method: "towards-zero", expected: ["9.0k", "-9.0k", "9.0k", "-9.0k"]},
    {method: "up", expected: ["9.1k", "-9.0k", "9.1k", "-9.0k"]},
    {method: "down", expected: ["9.0k", "-9.1k", "9.0k", "-9.1k"]},
    {method: "nearest", expected: ["9.0k", "-9.0k", "9.1k", "-9.1k"]},
    {method: "near", expected: ["9.0k", "-9.0k", "9.1k", "-9.1k"]},
    {method: "n", expected: ["9.0k", "-9.0k", "9.1k", "-9.1k"]}
  ] {
    let r = uu.invoke(s, "numfmt", ["--to=si", f"--round={case.method}", "--", "9001", "-9001", "9099", "-9099"])?
    uu.succeeds(r)
    uu.stdout_only(r, case.expected.join("\n") + "\n")
  }
}

# origin: uutils test_numfmt::test_round_with_to_unit
test test_uu_numfmt_round_with_to_unit { |ctx|
  let s = uu.scene(ctx)?
  for case in [
    {method: "from-zero", expected: ["6", "-6", "5.9", "-5.9", "5.86", "-5.86"]},
    {method: "towards-zero", expected: ["5", "-5", "5.8", "-5.8", "5.85", "-5.85"]},
    {method: "up", expected: ["6", "-5", "5.9", "-5.8", "5.86", "-5.85"]},
    {method: "down", expected: ["5", "-6", "5.8", "-5.9", "5.85", "-5.86"]},
    {method: "nearest", expected: ["6", "-6", "5.9", "-5.9", "5.86", "-5.86"]}
  ] {
    let r = uu.invoke(s, "numfmt", ["--to-unit=1024", f"--round={case.method}", "--", "6000", "-6000", "6000.0", "-6000.0", "6000.00", "-6000.00"])?
    uu.succeeds(r)
    uu.stdout_only(r, case.expected.join("\n") + "\n")
  }
}

# origin: uutils test_numfmt::test_should_calculate_implicit_padding_per_free_argument
test test_uu_numfmt_should_calculate_implicit_padding_per_free_argument { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=auto", "   1Ki", "        2K"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "  1024\n      2000\n")
}

# origin: uutils test_numfmt::test_should_calculate_implicit_padding_per_line
test test_uu_numfmt_should_calculate_implicit_padding_per_line { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=auto"], stdin: bytes.from_text("   1Ki\n        2K"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "  1024\n      2000")
}

# origin: uutils test_numfmt::test_should_convert_only_first_number_in_line
test test_uu_numfmt_should_convert_only_first_number_in_line { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=auto"], stdin: bytes.from_text("1Ki 2M 3G"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1024 2M 3G")
}

# origin: uutils test_numfmt::test_should_not_round_floats
test test_uu_numfmt_should_not_round_floats { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--", "0.99", "1.01", "1.1", "1.22", ".1", "-0.1"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "0.99\n1.01\n1.1\n1.22\n0.1\n-0.1\n")
}

# origin: uutils test_numfmt::test_should_preserve_trailing_zeros
test test_uu_numfmt_should_preserve_trailing_zeros { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["0.1000", "10.00"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "0.1000\n10.00\n")
}

# origin: uutils test_numfmt::test_should_report_invalid_empty_number_on_blank_stdin
test test_uu_numfmt_should_report_invalid_empty_number_on_blank_stdin { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=auto"], stdin: bytes.from_text("  \t  \n"))?
  uu.fails(r1)
  uu.stderr_is(r1, "numfmt: invalid number: ''\n")
}

# origin: uutils test_numfmt::test_should_report_invalid_empty_number_on_empty_stdin
test test_uu_numfmt_should_report_invalid_empty_number_on_empty_stdin { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=auto"], stdin: bytes.from_text("\n"))?
  uu.fails(r1)
  uu.stderr_is(r1, "numfmt: invalid number: ''\n")
}

# origin: uutils test_numfmt::test_should_report_invalid_number_with_interior_junk
test test_uu_numfmt_should_report_invalid_number_with_interior_junk { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=auto"], stdin: bytes.from_text("1x0K"))?
  uu.fails(r1)
  uu.stderr_is(r1, "numfmt: invalid suffix in input: '1x0K'\n")
}

# origin: uutils test_numfmt::test_should_report_invalid_number_with_sign_after_decimal
test test_uu_numfmt_should_report_invalid_number_with_sign_after_decimal { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--", "-0.-1"])?
  uu.fails_with_code(r1, 2)
  uu.stderr_is(r1, "numfmt: invalid number: '-0.-1'\n")
}

# origin: uutils test_numfmt::test_should_report_invalid_suffix_on_nan
test test_uu_numfmt_should_report_invalid_suffix_on_nan { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=auto"], stdin: bytes.from_text("NaN"))?
  uu.fails(r1)
  uu.stderr_is(r1, "numfmt: invalid number: 'NaN'\n")
}

# origin: uutils test_numfmt::test_should_skip_leading_space_from_stdin
test test_uu_numfmt_should_skip_leading_space_from_stdin { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=auto"], stdin: bytes.from_text(" 2Ki"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "2048")
  let r2 = uu.invoke(s, "numfmt", ["--from=auto"], stdin: bytes.from_text("\t1Ki\n  2K"))?
  uu.succeeds(r2)
  uu.stdout_is(r2, "1024\n2000")
}

# origin: uutils test_numfmt::test_should_succeed_if_range_out_of_bounds
test test_uu_numfmt_should_succeed_if_range_out_of_bounds { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "5-10", "1K 2K 3K 4K 5K 6K"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1K 2K 3K 4K 5000 6000\n")
}

# origin: uutils test_numfmt::test_should_succeed_if_selected_field_out_of_range
test test_uu_numfmt_should_succeed_if_selected_field_out_of_range { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=auto", "--field", "9", "1K 2K 3K"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1K 2K 3K\n")
}

# origin: uutils test_numfmt::test_si_format_precision_no_cap
test test_uu_numfmt_si_format_precision_no_cap { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--to=si", "--format=%.5f", "1234567"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1.23457M\n")
}

# origin: uutils test_numfmt::test_si_to_iec
test test_uu_numfmt_si_to_iec { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=si", "--to=iec", "15334263563K"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "14T\n")
}

# origin: uutils test_numfmt::test_suffix_hyphen_leading_as_separate_arg
test test_uu_numfmt_suffix_hyphen_leading_as_separate_arg { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--suffix", "-x"], stdin: bytes.from_text("5\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "5-x\n")
}

# origin: uutils test_numfmt::test_suffix_is_added_if_not_supplied
test test_uu_numfmt_suffix_is_added_if_not_supplied { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--suffix=TEST"], stdin: bytes.from_text("1000"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1000TEST")
}

# origin: uutils test_numfmt::test_suffix_is_only_applied_to_selected_field
test test_uu_numfmt_suffix_is_only_applied_to_selected_field { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--suffix=TEST", "--field=2"], stdin: bytes.from_text("1000 2000 3000"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1000 2000TEST 3000")
}

# origin: uutils test_numfmt::test_suffix_is_preserved
test test_uu_numfmt_suffix_is_preserved { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--suffix=TEST"], stdin: bytes.from_text("1000TEST"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1000TEST")
}

# origin: uutils test_numfmt::test_suffix_with_padding
test test_uu_numfmt_suffix_with_padding { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--suffix=pad", "--padding=12"], stdin: bytes.from_text("1000 2000 3000"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "     1000pad 2000 3000")
}

# origin: uutils test_numfmt::test_suffixes
test test_uu_numfmt_suffixes { |ctx|
  let s = uu.scene(ctx)?
  let valid_suffixes = ["K", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q", "k"]
  for c in ["ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz".byte_slice(i, length: 1) for i in range(52)] {
    let r = uu.invoke(s, "numfmt", ["--from=si", "--to=si", f"1{c}"])?
    if c in valid_suffixes {
      let suffix = if c == "K" { "k" } else { c }
      uu.succeeds(r)
      uu.stdout_only(r, f"1.0{suffix}\n")
    } else {
      uu.fails_with_code(r, 2)
      uu.stderr_only(r, f"numfmt: invalid suffix in input: '1{c}'\n")
    }
  }
}

# origin: uutils test_numfmt::test_to_auto_rejected_at_parse_time
test test_uu_numfmt_to_auto_rejected_at_parse_time { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--to=auto", "100"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "invalid argument 'auto' for '--to'")
}

# origin: uutils test_numfmt::test_to_iec
test test_uu_numfmt_to_iec { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--to=iec"], stdin: bytes.from_text("1024\n1153434\n107374182"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1.0K\n1.2M\n103M")
}

# origin: uutils test_numfmt::test_to_iec_i
test test_uu_numfmt_to_iec_i { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--to=iec-i"], stdin: bytes.from_text("1024\n1153434\n107374182"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1.0Ki\n1.2Mi\n103Mi")
}

# origin: uutils test_numfmt::test_to_iec_i_should_truncate_output
test test_uu_numfmt_to_iec_i_should_truncate_output { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "numfmt", "gnutest_iec_input.txt", "input")?
  uu.fixture(s, "numfmt", "gnutest_iec-i_result.txt", "expected")?
  let r1 = uu.invoke(s, "numfmt", ["--to=iec-i"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, uu.read(s, "expected")?)
}

# origin: uutils test_numfmt::test_to_iec_should_truncate_output
test test_uu_numfmt_to_iec_should_truncate_output { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "numfmt", "gnutest_iec_input.txt", "input")?
  uu.fixture(s, "numfmt", "gnutest_iec_result.txt", "expected")?
  let r1 = uu.invoke(s, "numfmt", ["--to=iec"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, uu.read(s, "expected")?)
}

# origin: uutils test_numfmt::test_to_si
test test_uu_numfmt_to_si { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--to=si"], stdin: bytes.from_text("1000\n1100000\n100000000"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1.0k\n1.1M\n100M")
}

# origin: uutils test_numfmt::test_to_si_should_truncate_output
test test_uu_numfmt_to_si_should_truncate_output { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "numfmt", "gnutest_si_input.txt", "input")?
  uu.fixture(s, "numfmt", "gnutest_si_result.txt", "expected")?
  let r1 = uu.invoke(s, "numfmt", ["--to=si"], stdin: uu.read(s, "input")?)?
  uu.succeeds(r1)
  uu.stdout_is_bytes(r1, uu.read(s, "expected")?)
}

# origin: uutils test_numfmt::test_to_unit
test test_uu_numfmt_to_unit { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--to-unit=512", "2048"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "4\n")
}

# origin: uutils test_numfmt::test_to_unit_prefix_selection
test test_uu_numfmt_to_unit_prefix_selection { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--to=iec-i", "--to-unit=885", "100000"])?
  uu.succeeds(r1)
  uu.stdout_is(r1, "113\n")
}

# origin: uutils test_numfmt::test_to_unit_with_unitless_small_value_uses_display_rounding
test test_uu_numfmt_to_unit_with_unitless_small_value_uses_display_rounding { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--to=iec", "--to-unit=689", "701"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "1\n")
  let r2 = uu.invoke(s, "numfmt", ["--to=si", "--to-unit=689", "701"])?
  uu.succeeds(r2)
  uu.stdout_only(r2, "1\n")
  let r3 = uu.invoke(s, "numfmt", ["--to=none", "--to-unit=689", "701"])?
  uu.succeeds(r3)
  uu.stdout_only(r3, "2\n")
}

# origin: uutils test_numfmt::test_to_unitless_small_values_use_display_rounding
test test_uu_numfmt_to_unitless_small_values_use_display_rounding { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--to=si", "--", "0.4", "0.5", "0.6", "1.4", "3.14", "-0.4", "-0.5", "-0.6", "-1.4"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0\n0\n1\n1\n3\n-0\n-0\n-1\n-1\n")
}

# origin: uutils test_numfmt::test_transform_with_suffix_on_input
test test_uu_numfmt_transform_with_suffix_on_input { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--suffix=b", "--to=si"], stdin: bytes.from_text("2000b"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "2.0kb")
}

# origin: uutils test_numfmt::test_transform_without_suffix_on_input
test test_uu_numfmt_transform_without_suffix_on_input { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--suffix=b", "--to=si"], stdin: bytes.from_text("2000"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "2.0kb")
}

# origin: uutils test_numfmt::test_unit_hyphen_leading_as_separate_arg
test test_uu_numfmt_unit_hyphen_leading_as_separate_arg { |ctx|
  let s = uu.scene(ctx)?
  for opt in ["--from", "--to"] {
    let r = uu.invoke(s, "numfmt", [opt, "-x"], stdin: b"")?
    uu.fails(r)
    uu.stderr_contains(r, "invalid argument '-x' for '--")
  }
}

# origin: uutils test_numfmt::test_unit_separator
test test_uu_numfmt_unit_separator { |ctx|
  let s = uu.scene(ctx)?
  for case in [
    {args: ["--to=si", "--unit-separator= ", "1000"], expected: "1.0 k\n"},
    {args: ["--to=iec", "--unit-separator= ", "1024"], expected: "1.0 K\n"},
    {args: ["--to=iec-i", "--unit-separator= ", "2048"], expected: "2.0 Ki\n"},
    {args: ["--to=si", "--unit-separator=__", "1000"], expected: "1.0__k\n"},
    {args: ["--to=si", "--unit-separator= ", "500"], expected: "500\n"},
  ] {
    let r = uu.invoke(s, "numfmt", case.args)?
    uu.succeeds(r)
    uu.stdout_only(r, case.expected)
  }
}

# origin: uutils test_numfmt::test_unit_separator_hyphen_leading_as_separate_arg
test test_uu_numfmt_unit_separator_hyphen_leading_as_separate_arg { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--to=si", "--unit-separator", "-"], stdin: bytes.from_text("1000\n"))?
  uu.succeeds(r1)
  uu.stdout_is(r1, "1.0-k\n")
}

# origin: uutils test_numfmt::test_unit_size_hyphen_leading_as_separate_arg
test test_uu_numfmt_unit_size_hyphen_leading_as_separate_arg { |ctx|
  let s = uu.scene(ctx)?
  for opt in ["--from-unit", "--to-unit"] {
    let r = uu.invoke(s, "numfmt", [opt, "-1"], stdin: b"")?
    uu.fails(r)
    uu.stderr_contains(r, "invalid unit size: '-1'")
  }
}

# origin: uutils test_numfmt::test_valid_but_forbidden_suffix
test test_uu_numfmt_valid_but_forbidden_suffix { |ctx|
  let s = uu.scene(ctx)?
  for number in ["12K", "12Ki"] {
    let r = uu.invoke(s, "numfmt", [number])?
    uu.fails_with_code(r, 2)
    uu.stderr_contains(r, f"rejecting suffix in input: '{number}' (consider using --from)")
  }
}

# origin: uutils test_numfmt::test_whitespace_mode_parses_custom_unit_separator_inputs
test test_uu_numfmt_whitespace_mode_parses_custom_unit_separator_inputs { |ctx|
  let s = uu.scene(ctx)?
  let r1 = uu.invoke(s, "numfmt", ["--from=iec", "--unit-separator=::"], stdin: bytes.from_text("4::K\n"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "4096\n")
  let r2 = uu.invoke(s, "numfmt", ["--from=iec", "--unit-separator= "], stdin: bytes.from_text("4 K\n"))?
  uu.succeeds(r2)
  uu.stdout_only(r2, "4096\n")
}

# origin: uutils test_numfmt::test_write_error_is_reported_and_fatal
test test_uu_numfmt_write_error_is_reported_and_fatal { |ctx|
  let s = uu.scene(ctx)?
  for extra in [["--to=si"], ["--invalid=ignore"]] {
    let r = uu.invoke(s, "numfmt", extra, stdin: b"81920\n4096\n1024\n", stdout: p"/dev/full")?
    uu.fails_with_code(r, 1)
    uu.stderr_is(r, "numfmt: write error: No space left on device\n")
  }
}
